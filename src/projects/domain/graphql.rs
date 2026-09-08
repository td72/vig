//! Board fetch straight over the GraphQL API (`gh api graphql`), asking
//! only for what the page renders. `gh project field-list` / `item-list`
//! request every field value of every item at a fixed page size, which
//! costs ~100 points per call; these queries cost about one point per
//! hundred item × field pairs (a fields + 3-item probe measured 1 point).
//!
//! Two requests per board: the project's fields, saved views and item
//! count, then the items in pages of up to 100 sized to that count, each
//! item with as many field values as the project has fields. The
//! responses are reshaped into the `gh … --format json` layout so the
//! rest of the page ([`ProjectItem`] / [`ProjectField`]) is untouched.

use crate::projects::domain::client::ITEM_LIMIT;
use crate::projects::domain::types::{
    item_key, Board, ProjectField, ProjectItem, ProjectView, ViewLayout, ViewSort,
};
use serde_json::{json, Value};

/// Items per page (GitHub's maximum for `items(first:)`).
const PAGE: usize = 100;
/// Fields requested (GitHub's maximum for `fields(first:)`; a project
/// cannot define more).
const FIELD_CAP: usize = 100;

const VIEWS_SELECTION: &str = "views(first: 20) { nodes { \
    name number layout filter \
    groupByFields(first: 5) { nodes { ... on ProjectV2FieldCommon { name } } } \
    verticalGroupByFields(first: 5) { nodes { ... on ProjectV2FieldCommon { name } } } \
    sortByFields(first: 5) { nodes { direction field { ... on ProjectV2FieldCommon { name } } } } \
    fields(first: 30) { nodes { ... on ProjectV2FieldCommon { name } } } } }";

/// Fields, saved views and the item count of a project.
fn meta_query(root: &str) -> String {
    format!(
        "query($login: String!, $number: Int!) {{ {root}(login: $login) {{ projectV2(number: $number) {{ \
           fields(first: {FIELD_CAP}) {{ nodes {{ __typename \
             ... on ProjectV2FieldCommon {{ id name }} \
             ... on ProjectV2SingleSelectField {{ options {{ id name }} }} }} }} \
           {VIEWS_SELECTION} \
           items {{ totalCount }} }} }} \
         rateLimit {{ cost remaining }} }}"
    )
}

/// One page of items with their field values. `values_per_item` is the
/// project's field count (an item cannot carry more values than that),
/// which keeps the cost proportional to what the board really has.
fn items_query(root: &str, values_per_item: usize) -> String {
    let values_per_item = values_per_item.clamp(1, FIELD_CAP);
    format!(
        "query($login: String!, $number: Int!, $first: Int!, $after: String) {{ \
         {root}(login: $login) {{ projectV2(number: $number) {{ \
           items(first: $first, after: $after) {{ \
             pageInfo {{ hasNextPage endCursor }} \
             nodes {{ id type \
               content {{ __typename \
                 ... on Issue {{ number title url body repository {{ nameWithOwner }} }} \
                 ... on PullRequest {{ number title url body repository {{ nameWithOwner }} }} \
                 ... on DraftIssue {{ title body }} }} \
               fieldValues(first: {values_per_item}) {{ nodes {{ __typename \
                 ... on ProjectV2ItemFieldTextValue {{ text field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldNumberValue {{ number field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldDateValue {{ date field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldSingleSelectValue {{ name field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldIterationValue {{ title startDate duration iterationId field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldLabelValue {{ labels(first: 20) {{ nodes {{ name }} }} field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldUserValue {{ users(first: 10) {{ nodes {{ login }} }} field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldMilestoneValue {{ milestone {{ title description dueOn }} field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldRepositoryValue {{ repository {{ url nameWithOwner }} field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
                 ... on ProjectV2ItemFieldPullRequestValue {{ pullRequests(first: 10) {{ nodes {{ url }} }} field {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
               }} }} }} }} }} }} \
         rateLimit {{ cost remaining }} }}"
    )
}

/// Run a query and return `data.<root>.projectV2`, reporting the quota.
fn run(root: &str, query: &str, vars: &[(&str, String)]) -> Result<Value, String> {
    let mut args: Vec<String> = vec![
        "api".into(),
        "graphql".into(),
        "-f".into(),
        format!("query={query}"),
    ];
    for (k, v) in vars {
        // `Int!` variables go through -F (typed), `String` ones through -f
        // — by name, so a numeric-looking login or cursor stays a string.
        let flag = if matches!(*k, "number" | "first") {
            "-F"
        } else {
            "-f"
        };
        args.push(flag.into());
        args.push(format!("{k}={v}"));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let resp: Value =
        crate::github::domain::client::run_gh_json(&refs, "gh api graphql (project) failed")?;
    if let Some(left) = resp
        .pointer("/data/rateLimit/remaining")
        .and_then(Value::as_u64)
    {
        crate::core::api_budget::note(left);
    }
    if let Some(errors) = resp.get("errors").and_then(Value::as_array) {
        let msg = errors
            .iter()
            .filter_map(|e| e.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("GraphQL: {msg}"));
    }
    resp.pointer(&format!("/data/{root}/projectV2"))
        .filter(|v| !v.is_null())
        .cloned()
        .ok_or_else(|| format!("project #{}: not found under {root}", vars_number(vars)))
}

fn vars_number(vars: &[(&str, String)]) -> String {
    vars.iter()
        .find(|(k, _)| *k == "number")
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// Fields, items and saved views of a project. `owner_kind` is `User` /
/// `Organization` from the linked project's `resourcePath`; anything else
/// tries the user root first, then the organization one.
pub fn fetch_board(owner: &str, owner_kind: &str, number: u64) -> Result<Board, String> {
    match owner_kind {
        "User" => fetch_as("user", owner, number),
        "Organization" => fetch_as("organization", owner, number),
        _ => fetch_as("user", owner, number).or_else(|_| fetch_as("organization", owner, number)),
    }
}

fn fetch_as(root: &str, owner: &str, number: u64) -> Result<Board, String> {
    let base = [("login", owner.to_string()), ("number", number.to_string())];
    let meta = run(root, &meta_query(root), &base)?;
    let fields = fields_from(meta.pointer("/fields/nodes").unwrap_or(&Value::Null));
    let views = views_from(meta.get("views").unwrap_or(&Value::Null));
    let total_count = meta
        .pointer("/items/totalCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let wanted = (total_count as usize).min(ITEM_LIMIT);
    let mut items: Vec<ProjectItem> = Vec::with_capacity(wanted);
    let mut after: Option<String> = None;
    while items.len() < wanted {
        let first = (wanted - items.len()).min(PAGE);
        let mut vars = base.to_vec();
        vars.push(("first", first.to_string()));
        if let Some(cursor) = &after {
            vars.push(("after", cursor.clone()));
        }
        let page = run(root, &items_query(root, fields.len()), &vars)?;
        let page_items = items_from(page.pointer("/items/nodes").unwrap_or(&Value::Null));
        if page_items.is_empty() {
            break;
        }
        items.extend(page_items);
        let has_next = page
            .pointer("/items/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        after = page
            .pointer("/items/pageInfo/endCursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        if !has_next || after.is_none() {
            break;
        }
    }
    Ok(Board {
        number,
        fields,
        items,
        total_count,
        views,
    })
}

/// `fields.nodes` → the `gh project field-list` shape.
pub(crate) fn fields_from(nodes: &Value) -> Vec<ProjectField> {
    nodes
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| {
                    let name = f.get("name")?.as_str()?.to_string();
                    let kind = f
                        .get("__typename")
                        .and_then(Value::as_str)
                        .unwrap_or("ProjectV2Field")
                        .to_string();
                    let options = f
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|o| {
                            o.iter()
                                .filter_map(|opt| {
                                    Some(crate::projects::domain::types::FieldOption {
                                        id: opt.get("id")?.as_str().unwrap_or("").to_string(),
                                        name: opt.get("name")?.as_str()?.to_string(),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(ProjectField {
                        id: f
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        name,
                        kind,
                        options,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `views` → [`ProjectView`]s (unknown layouts are skipped).
pub(crate) fn views_from(views: &Value) -> Vec<ProjectView> {
    let names = |v: &Value| -> Vec<String> {
        v.get("nodes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.get("name").and_then(Value::as_str))
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    views
        .get("nodes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    Some(ProjectView {
                        number: v.get("number")?.as_u64()?,
                        name: v
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        layout: ViewLayout::parse(v.get("layout").and_then(Value::as_str)?)?,
                        filter: v
                            .get("filter")
                            .and_then(Value::as_str)
                            .filter(|f| !f.trim().is_empty())
                            .map(str::to_string),
                        group_by: names(v.get("groupByFields").unwrap_or(&Value::Null)),
                        vertical_group_by: names(
                            v.get("verticalGroupByFields").unwrap_or(&Value::Null),
                        ),
                        sort_by: v
                            .pointer("/sortByFields/nodes")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(|s| {
                                        let field = s.pointer("/field/name")?.as_str()?.to_string();
                                        (!field.is_empty()).then(|| ViewSort {
                                            field,
                                            desc: s.get("direction").and_then(Value::as_str)
                                                == Some("DESC"),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        visible_fields: names(v.get("fields").unwrap_or(&Value::Null)),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `items.nodes` → [`ProjectItem`]s in the `gh project item-list` shape:
/// `title` / `status` / `content` explicit, every other field value under
/// its lowercased-first-letter key.
pub(crate) fn items_from(nodes: &Value) -> Vec<ProjectItem> {
    nodes
        .as_array()
        .map(|a| a.iter().filter_map(item_from).collect())
        .unwrap_or_default()
}

fn item_from(node: &Value) -> Option<ProjectItem> {
    let mut obj = serde_json::Map::new();
    obj.insert("id".into(), node.get("id").cloned().unwrap_or(Value::Null));
    if let Some(content) = node.get("content").filter(|c| !c.is_null()) {
        let kind = content
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut c = serde_json::Map::new();
        c.insert("type".into(), json!(kind));
        for key in ["title", "number", "url", "body"] {
            if let Some(v) = content.get(key) {
                c.insert(key.into(), v.clone());
            }
        }
        if let Some(repo) = content.pointer("/repository/nameWithOwner") {
            c.insert("repository".into(), repo.clone());
        }
        obj.insert("content".into(), Value::Object(c));
    } else {
        obj.insert("content".into(), Value::Null);
    }
    for value in node
        .pointer("/fieldValues/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(field) = value.pointer("/field/name").and_then(Value::as_str) else {
            continue;
        };
        let kind = value
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("");
        let converted: Value = match kind {
            "ProjectV2ItemFieldTextValue" => value.get("text").cloned().unwrap_or(Value::Null),
            "ProjectV2ItemFieldNumberValue" => value.get("number").cloned().unwrap_or(Value::Null),
            "ProjectV2ItemFieldDateValue" => value.get("date").cloned().unwrap_or(Value::Null),
            "ProjectV2ItemFieldSingleSelectValue" => {
                value.get("name").cloned().unwrap_or(Value::Null)
            }
            "ProjectV2ItemFieldIterationValue" => json!({
                "title": value.get("title"),
                "startDate": value.get("startDate"),
                "duration": value.get("duration"),
                "iterationId": value.get("iterationId"),
            }),
            "ProjectV2ItemFieldLabelValue" => collect(value, "/labels/nodes", "name"),
            "ProjectV2ItemFieldUserValue" => collect(value, "/users/nodes", "login"),
            "ProjectV2ItemFieldMilestoneValue" => {
                value.get("milestone").cloned().unwrap_or(Value::Null)
            }
            "ProjectV2ItemFieldRepositoryValue" => value
                .pointer("/repository/url")
                .cloned()
                .unwrap_or(Value::Null),
            "ProjectV2ItemFieldPullRequestValue" => collect(value, "/pullRequests/nodes", "url"),
            _ => continue,
        };
        if converted.is_null() {
            continue;
        }
        match field {
            "Title" => {
                obj.insert("title".into(), converted);
            }
            "Status" => {
                obj.insert("status".into(), converted);
            }
            name => {
                obj.insert(item_key(name), converted);
            }
        }
    }
    if !obj.contains_key("title") {
        if let Some(t) = node.pointer("/content/title") {
            obj.insert("title".into(), t.clone());
        }
    }
    serde_json::from_value(Value::Object(obj)).ok()
}

/// The `key` of every node at `pointer`, as a JSON array of strings.
fn collect(value: &Value, pointer: &str, key: &str) -> Value {
    Value::Array(
        value
            .pointer(pointer)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|n| n.get(key).cloned()).collect())
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r###"{
      "fields": {"nodes": [
        {"__typename":"ProjectV2Field","id":"F1","name":"Title"},
        {"__typename":"ProjectV2SingleSelectField","id":"F3","name":"Status","options":[{"id":"a","name":"Todo"},{"id":"c","name":"Done"}]},
        {"__typename":"ProjectV2Field","id":"F9","name":"Start date"},
        {"__typename":"ProjectV2IterationField","id":"F8","name":"Iteration"}
      ]},
      "views": {"nodes": [
        {"name":"By status","number":2,"layout":"BOARD_LAYOUT","filter":"-status:Done",
         "groupByFields":{"nodes":[]},"verticalGroupByFields":{"nodes":[{"name":"Status"}]},
         "sortByFields":{"nodes":[{"direction":"DESC","field":{"name":"Title"}}]},
         "fields":{"nodes":[{"name":"Title"},{"name":"Status"}]}},
        {"name":"Weird","number":9,"layout":"HOLOGRAM_LAYOUT"}
      ]},
      "items": {"totalCount": 42}
    }"###;

    const ITEMS: &str = r###"{"nodes": [
      {"id":"PVTI_1","type":"ISSUE",
       "content":{"__typename":"Issue","number":114,"title":"Config: explicit page slots","url":"https://github.com/td72/vig/issues/114","body":"## Problem","repository":{"nameWithOwner":"td72/vig"}},
       "fieldValues":{"nodes":[
         {"__typename":"ProjectV2ItemFieldRepositoryValue","repository":{"url":"https://github.com/td72/vig","nameWithOwner":"td72/vig"},"field":{"name":"Repository"}},
         {"__typename":"ProjectV2ItemFieldLabelValue","labels":{"nodes":[{"name":"enhancement"}]},"field":{"name":"Labels"}},
         {"__typename":"ProjectV2ItemFieldPullRequestValue","pullRequests":{"nodes":[{"url":"https://github.com/td72/vig/pull/117"}]},"field":{"name":"Linked pull requests"}},
         {"__typename":"ProjectV2ItemFieldUserValue","users":{"nodes":[{"login":"td72"}]},"field":{"name":"Assignees"}},
         {"__typename":"ProjectV2ItemFieldTextValue","text":"Config: explicit page slots","field":{"name":"Title"}},
         {"__typename":"ProjectV2ItemFieldSingleSelectValue","name":"Done","field":{"name":"Status"}},
         {"__typename":"ProjectV2ItemFieldNumberValue","number":3,"field":{"name":"Estimate"}},
         {"__typename":"ProjectV2ItemFieldDateValue","date":"2026-09-11","field":{"name":"Target date"}},
         {"__typename":"ProjectV2ItemFieldIterationValue","title":"Sprint 3","startDate":"2026-08-24","duration":14,"iterationId":"it3","field":{"name":"Iteration"}},
         {"__typename":"ProjectV2ItemFieldMilestoneValue","milestone":{"title":"v1","description":null,"dueOn":null},"field":{"name":"Milestone"}},
         {"__typename":"ProjectV2ItemFieldMysteryValue","field":{"name":"Future"}}
       ]}},
      {"id":"PVTI_2","type":"DRAFT_ISSUE",
       "content":{"__typename":"DraftIssue","title":"Record the demo","body":"Draft body"},
       "fieldValues":{"nodes":[
         {"__typename":"ProjectV2ItemFieldTextValue","text":"Record the demo","field":{"name":"Title"}}
       ]}},
      {"id":"PVTI_3","type":"REDACTED","content":null,"fieldValues":{"nodes":[]}}
    ]}"###;

    #[test]
    fn fields_take_the_field_list_shape() {
        let meta: Value = serde_json::from_str(META).unwrap();
        let fields = fields_from(&meta["fields"]["nodes"]);
        assert_eq!(fields.len(), 4);
        let status = fields.iter().find(|f| f.name == "Status").unwrap();
        assert!(status.is_status());
        assert_eq!(status.kind, "ProjectV2SingleSelectField");
        assert_eq!(
            status
                .options
                .iter()
                .map(|o| o.name.as_str())
                .collect::<Vec<_>>(),
            ["Todo", "Done"]
        );
        assert_eq!(fields[3].kind, "ProjectV2IterationField");
    }

    #[test]
    fn views_parse_and_skip_unknown_layouts() {
        let meta: Value = serde_json::from_str(META).unwrap();
        let views = views_from(&meta["views"]);
        assert_eq!(views.len(), 1);
        let v = &views[0];
        assert_eq!(v.name, "By status");
        assert_eq!(v.layout, ViewLayout::Board);
        assert_eq!(v.filter.as_deref(), Some("-status:Done"));
        assert_eq!(v.vertical_group_by, vec!["Status"]);
        assert!(v.sort_by[0].desc);
        assert_eq!(v.visible_fields, vec!["Title", "Status"]);
        assert_eq!(meta["items"]["totalCount"], 42);
    }

    #[test]
    fn items_take_the_item_list_shape() {
        let nodes: Value = serde_json::from_str(ITEMS).unwrap();
        let items = items_from(&nodes["nodes"]);
        assert_eq!(items.len(), 3);
        let issue = &items[0];
        assert_eq!(issue.id, "PVTI_1");
        assert_eq!(issue.title(), "Config: explicit page slots");
        assert_eq!(issue.status.as_deref(), Some("Done"));
        assert_eq!(issue.number(), Some(114));
        assert_eq!(issue.repository(), Some("td72/vig"));
        assert_eq!(issue.url(), Some("https://github.com/td72/vig/issues/114"));
        assert_eq!(issue.body(), "## Problem");
        assert_eq!(issue.assignees(), vec!["td72"]);
        assert_eq!(issue.field_text("labels").as_deref(), Some("enhancement"));
        assert_eq!(issue.field_text("estimate").as_deref(), Some("3"));
        assert_eq!(
            issue.field_text("target date").as_deref(),
            Some("2026-09-11")
        );
        assert_eq!(issue.field_text("milestone").as_deref(), Some("v1"));
        assert_eq!(
            issue.field_text("iteration").as_deref(),
            Some("Sprint 3 (2026-08-24)")
        );
        assert_eq!(
            issue.field_text("linked pull requests").as_deref(),
            Some("https://github.com/td72/vig/pull/117")
        );
        assert_eq!(
            issue.field_text("repository").as_deref(),
            Some("https://github.com/td72/vig")
        );
        assert!(!issue.fields.contains_key("future"));
        // Draft and redacted items.
        assert_eq!(
            items[1].kind(),
            crate::projects::domain::types::ItemKind::Draft
        );
        assert_eq!(items[1].title(), "Record the demo");
        assert_eq!(items[1].body(), "Draft body");
        assert!(items[2].content.is_none());
        assert_eq!(items[2].id, "PVTI_3");
    }

    #[test]
    fn queries_mention_every_value_type_and_page_by_cursor() {
        let q = items_query("user", 15);
        for ty in [
            "TextValue",
            "NumberValue",
            "DateValue",
            "SingleSelectValue",
            "IterationValue",
            "LabelValue",
            "UserValue",
            "MilestoneValue",
            "RepositoryValue",
            "PullRequestValue",
        ] {
            assert!(q.contains(&format!("ProjectV2ItemField{ty}")), "{ty}");
        }
        assert!(q.contains("items(first: $first, after: $after)"));
        assert!(q.contains("fieldValues(first: 15)"));
        // Clamped to GitHub's maximum, never zero.
        assert!(items_query("user", 500).contains("fieldValues(first: 100)"));
        assert!(items_query("user", 0).contains("fieldValues(first: 1)"));
        assert!(meta_query("user").contains("fields(first: 100)"));
        assert!(q.contains("rateLimit { cost remaining }"));
        assert!(meta_query("organization")
            .starts_with("query($login: String!, $number: Int!) { organization(login: $login)"));
    }
}
