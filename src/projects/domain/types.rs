//! Data shapes read from `gh project … --format json`.
//!
//! `gh project item-list` emits one object per item whose keys are the
//! project's field names with the first letter lowercased (`status`,
//! `priority`, `linked pull requests`, …). Only `id`, `title`, `status` and
//! `content` are modelled explicitly; every other field lands in
//! [`ProjectItem::fields`] so custom fields (single select, text, number,
//! date, iteration) show up without a schema change.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Column shown for items whose `Status` is unset.
pub const NO_STATUS: &str = "No status";

// === gh repo view ===

/// `gh repo view --json owner,projectsV2`: the owner whose projects are
/// listed and the projects linked to the current repository.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepoInfo {
    pub owner: RepoOwner,
    #[serde(rename = "projectsV2", default)]
    pub projects_v2: LinkedProjects,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepoOwner {
    #[serde(default)]
    pub login: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LinkedProjects {
    #[serde(rename = "Nodes", default)]
    pub nodes: Vec<LinkedProject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinkedProject {
    pub number: u64,
}

impl RepoInfo {
    pub fn linked_numbers(&self) -> Vec<u64> {
        self.projects_v2.nodes.iter().map(|p| p.number).collect()
    }
}

// === gh project list ===

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectList {
    #[serde(default)]
    pub projects: Vec<Project>,
}

/// One project of `gh project list --format json`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Project {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub public: bool,
    #[serde(rename = "shortDescription", default)]
    pub short_description: String,
    #[serde(default)]
    pub items: Count,
    #[serde(default)]
    pub fields: Count,
    #[serde(default)]
    pub owner: ProjectOwner,
    /// `updatedAt` (ISO 8601), filled in from a GraphQL query: the
    /// `gh project list` JSON does not carry it.
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    /// Linked to the repository vig runs in (listed first, marked).
    #[serde(default)]
    pub linked: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Count {
    #[serde(rename = "totalCount", default)]
    pub total_count: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProjectOwner {
    #[serde(default)]
    pub login: String,
    #[serde(rename = "type", default)]
    pub kind: String,
}

/// Linked projects first (keeping their relative order), then the rest.
pub fn order_projects(mut projects: Vec<Project>, linked: &[u64]) -> Vec<Project> {
    for p in &mut projects {
        p.linked = linked.contains(&p.number);
    }
    projects.sort_by_key(|p| !p.linked);
    projects
}

/// What the project list pane keeps on disk between runs.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProjectListCache {
    pub owner: String,
    #[serde(default)]
    pub linked: Vec<u64>,
    #[serde(default)]
    pub projects: Vec<Project>,
}

// === gh project field-list ===

#[derive(Debug, Clone, Deserialize)]
pub struct FieldList {
    #[serde(default)]
    pub fields: Vec<ProjectField>,
}

/// One field of `gh project field-list --format json`. `kind` is
/// `ProjectV2Field`, `ProjectV2SingleSelectField` or
/// `ProjectV2IterationField`; only single selects carry `options`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectField {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub options: Vec<FieldOption>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FieldOption {
    #[serde(default)]
    pub id: String,
    pub name: String,
}

impl ProjectField {
    /// The key `gh project item-list` uses for this field: the name with
    /// its first letter lowercased (`Linked pull requests` →
    /// `linked pull requests`).
    pub fn item_key(&self) -> String {
        item_key(&self.name)
    }

    pub fn is_status(&self) -> bool {
        self.name == "Status" && self.kind == "ProjectV2SingleSelectField"
    }
}

pub fn item_key(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Fields GitHub manages itself; they are either on the item's content
/// (title, assignees, labels, …) or never emitted by `item-list`. The
/// table mode shows the remaining ones — `Status` and the custom fields.
const BUILTIN_FIELDS: &[&str] = &[
    "Title",
    "Assignees",
    "Labels",
    "Linked pull requests",
    "Milestone",
    "Repository",
    "Reviewers",
    "Parent issue",
    "Sub-issues progress",
    "Tracked by",
    "Tracks",
    "Created",
    "Updated",
    "Closed",
];

/// The fields shown as table columns, in the project's field order.
pub fn table_fields(fields: &[ProjectField]) -> Vec<&ProjectField> {
    fields
        .iter()
        .filter(|f| !BUILTIN_FIELDS.contains(&f.name.as_str()))
        .collect()
}

// === gh project item-list ===

#[derive(Debug, Clone, Deserialize)]
pub struct ItemList {
    #[serde(default)]
    pub items: Vec<ProjectItem>,
    #[serde(rename = "totalCount", default)]
    pub total_count: u64,
}

/// One item of `gh project item-list --format json`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// The `Status` single select, `None` when unset.
    #[serde(default)]
    pub status: Option<String>,
    /// The issue / PR / draft behind the item (`null` for redacted items).
    #[serde(default)]
    pub content: Option<ItemContent>,
    /// Every other field of the item, keyed the way `gh` names them.
    #[serde(flatten, default)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ItemContent {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub number: Option<u64>,
    /// `owner/repo` for issues and PRs.
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// Draft issues carry their body here; issues and PRs too.
    #[serde(default)]
    pub body: Option<String>,
}

/// Coarse item type, for icons and the detail pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Issue,
    PullRequest,
    Draft,
    Other,
}

impl ItemKind {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Issue => "●",
            Self::PullRequest => "⇅",
            Self::Draft => "✎",
            Self::Other => "?",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PullRequest => "pull request",
            Self::Draft => "draft",
            Self::Other => "item",
        }
    }
}

impl ProjectItem {
    pub fn kind(&self) -> ItemKind {
        match self.content.as_ref().map(|c| c.kind.as_str()) {
            Some("Issue") => ItemKind::Issue,
            Some("PullRequest") => ItemKind::PullRequest,
            Some("DraftIssue") => ItemKind::Draft,
            _ => ItemKind::Other,
        }
    }

    pub fn number(&self) -> Option<u64> {
        self.content.as_ref().and_then(|c| c.number)
    }

    pub fn repository(&self) -> Option<&str> {
        self.content.as_ref().and_then(|c| c.repository.as_deref())
    }

    pub fn url(&self) -> Option<&str> {
        self.content.as_ref().and_then(|c| c.url.as_deref())
    }

    pub fn body(&self) -> &str {
        self.content
            .as_ref()
            .and_then(|c| c.body.as_deref())
            .unwrap_or("")
    }

    pub fn title(&self) -> &str {
        if self.title.is_empty() {
            self.content
                .as_ref()
                .map(|c| c.title.as_str())
                .unwrap_or("")
        } else {
            &self.title
        }
    }

    /// Assignee logins (the `assignees` field, absent when unassigned).
    pub fn assignees(&self) -> Vec<String> {
        match self.fields.get("assignees") {
            Some(serde_json::Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The display text of a field by its item key (`status`, `priority`,
    /// …); `None` when the item has no value for it.
    pub fn field_text(&self, key: &str) -> Option<String> {
        if key == "status" {
            return self.status.clone();
        }
        if key == "title" {
            return Some(self.title().to_string());
        }
        self.fields.get(key).and_then(value_text)
    }

    /// Every field value of the item as `(key, text)`, `Status` first,
    /// then in `fields` order (falling back to alphabetical for keys the
    /// project does not list any more).
    pub fn field_values(&self, fields: &[ProjectField]) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(s) = &self.status {
            out.push(("Status".to_string(), s.clone()));
        }
        let mut seen: Vec<String> = vec!["status".into(), "title".into()];
        for f in fields {
            let key = f.item_key();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key.clone());
            if let Some(text) = self.field_text(&key) {
                out.push((f.name.clone(), text));
            }
        }
        for (key, value) in &self.fields {
            if seen.contains(key) {
                continue;
            }
            if let Some(text) = value_text(value) {
                out.push((capitalize(key), text));
            }
        }
        out
    }
}

/// Render a field value the way it reads best: iterations by title,
/// milestones by title, lists joined, numbers without trailing zeros.
pub fn value_text(v: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().filter_map(value_text).collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(", "))
            }
        }
        Value::Object(o) => {
            let title = o.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let start = o.get("startDate").and_then(|t| t.as_str());
            match (title.is_empty(), start) {
                (false, Some(start)) => Some(format!("{title} ({start})")),
                (false, None) => Some(title.to_string()),
                (true, _) => o
                    .values()
                    .filter_map(value_text)
                    .next()
                    .filter(|s| !s.is_empty()),
            }
        }
    }
}

fn capitalize(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// === Board ===

/// A project's board: its fields and items, with the truncation flag of
/// the `item-list --limit`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Board {
    pub number: u64,
    #[serde(default)]
    pub fields: Vec<ProjectField>,
    #[serde(default)]
    pub items: Vec<ProjectItem>,
    #[serde(default)]
    pub total_count: u64,
}

impl Board {
    /// More items exist than `item-list --limit` returned.
    pub fn truncated(&self) -> bool {
        (self.items.len() as u64) < self.total_count
    }

    pub fn status_field(&self) -> Option<&ProjectField> {
        self.fields.iter().find(|f| f.is_status())
    }

    /// Kanban columns: one per `Status` option in GitHub's order, then any
    /// status the options do not list, then [`NO_STATUS`] when some item
    /// has none. Each column holds indices into `items`.
    pub fn columns(&self) -> Vec<Column> {
        let mut columns: Vec<Column> = self
            .status_field()
            .map(|f| f.options.iter().map(|o| Column::new(&o.name)).collect())
            .unwrap_or_default();
        let mut no_status = Column::new(NO_STATUS);
        for (idx, item) in self.items.iter().enumerate() {
            match item.status.as_deref().filter(|s| !s.is_empty()) {
                Some(status) => {
                    if let Some(col) = columns.iter_mut().find(|c| c.name == status) {
                        col.items.push(idx);
                    } else {
                        let mut col = Column::new(status);
                        col.items.push(idx);
                        columns.push(col);
                    }
                }
                None => no_status.items.push(idx),
            }
        }
        if !no_status.items.is_empty() {
            columns.push(no_status);
        }
        columns
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub items: Vec<usize>,
}

impl Column {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            items: Vec::new(),
        }
    }
}

// === Table ===

/// A table column: the header and how to read a cell off an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableColumn {
    Number,
    Title,
    Assignees,
    Field { name: String, key: String },
}

impl TableColumn {
    pub fn header(&self) -> &str {
        match self {
            Self::Number => "#",
            Self::Title => "Title",
            Self::Assignees => "Assignees",
            Self::Field { name, .. } => name,
        }
    }

    pub fn cell(&self, item: &ProjectItem) -> String {
        match self {
            Self::Number => item.number().map(|n| format!("#{n}")).unwrap_or_default(),
            Self::Title => item.title().to_string(),
            Self::Assignees => item.assignees().join(", "),
            Self::Field { key, .. } => item.field_text(key).unwrap_or_default(),
        }
    }
}

/// `#`, `Title`, `Assignees`, then the project's own fields.
pub fn table_columns(fields: &[ProjectField]) -> Vec<TableColumn> {
    let mut cols = vec![
        TableColumn::Number,
        TableColumn::Title,
        TableColumn::Assignees,
    ];
    cols.extend(
        table_fields(fields)
            .into_iter()
            .map(|f| TableColumn::Field {
                name: f.name.clone(),
                key: f.item_key(),
            }),
    );
    cols
}

/// Item indices sorted by `column`: numerically when every non-empty cell
/// is a number (`#12`, `3.5`), case-insensitively otherwise; empty cells
/// last; ties keep the board order. `Status` sorts by option order.
pub fn sort_items(items: &[ProjectItem], column: &TableColumn, board: &Board) -> Vec<usize> {
    let option_rank = |s: &str| -> Option<usize> {
        board
            .status_field()
            .and_then(|f| f.options.iter().position(|o| o.name == s))
    };
    let cells: Vec<String> = items.iter().map(|i| column.cell(i)).collect();
    let numeric = cells
        .iter()
        .filter(|c| !c.is_empty())
        .all(|c| c.trim_start_matches('#').parse::<f64>().is_ok());
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let (ca, cb) = (&cells[a], &cells[b]);
        match (ca.is_empty(), cb.is_empty()) {
            (true, true) => return std::cmp::Ordering::Equal,
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        }
        if matches!(column, TableColumn::Field { key, .. } if key == "status") {
            // Options in GitHub order, statuses the field no longer lists after them.
            let (ra, rb) = (
                option_rank(ca).unwrap_or(usize::MAX),
                option_rank(cb).unwrap_or(usize::MAX),
            );
            if ra != rb {
                return ra.cmp(&rb);
            }
        }
        if numeric {
            let na: f64 = ca.trim_start_matches('#').parse().unwrap_or(0.0);
            let nb: f64 = cb.trim_start_matches('#').parse().unwrap_or(0.0);
            na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            ca.to_lowercase().cmp(&cb.to_lowercase())
        }
    });
    order
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const FIELDS_JSON: &str = r#"{"fields":[
      {"id":"F1","name":"Title","type":"ProjectV2Field"},
      {"id":"F2","name":"Assignees","type":"ProjectV2Field"},
      {"id":"F3","name":"Status","options":[{"id":"a","name":"Todo"},{"id":"b","name":"In Progress"},{"id":"c","name":"Done"}],"type":"ProjectV2SingleSelectField"},
      {"id":"F4","name":"Labels","type":"ProjectV2Field"},
      {"id":"F5","name":"Linked pull requests","type":"ProjectV2Field"},
      {"id":"F6","name":"Priority","options":[{"id":"p1","name":"P1"},{"id":"p2","name":"P2"}],"type":"ProjectV2SingleSelectField"},
      {"id":"F7","name":"Estimate","type":"ProjectV2Field"},
      {"id":"F8","name":"Iteration","type":"ProjectV2IterationField"},
      {"id":"F9","name":"Start date","type":"ProjectV2Field"}
    ],"totalCount":9}"#;

    pub(crate) const ITEMS_JSON: &str = r###"{"items":[
      {"content":{"body":"## Problem\n\nText","number":114,"repository":"td72/vig","title":"Config: explicit page slots","type":"Issue","url":"https://github.com/td72/vig/issues/114"},
       "id":"I1","labels":["enhancement"],"linked pull requests":["https://github.com/td72/vig/pull/117"],"repository":"https://github.com/td72/vig","status":"Done","title":"Config: explicit page slots","assignees":["td72"],"estimate":3,"priority":"P1"},
      {"content":{"body":"","number":124,"repository":"td72/vig","title":"Fold the Actions page","type":"PullRequest","url":"https://github.com/td72/vig/pull/124"},
       "id":"I2","repository":"https://github.com/td72/vig","status":"In Progress","title":"Fold the Actions page","estimate":1.5,"iteration":{"title":"Sprint 3","startDate":"2026-08-24","duration":14,"iterationId":"it3"},"start date":"2026-08-25"},
      {"content":{"body":"Draft item used by the vig demo.","id":"DI_1","title":"Record the Projects demo tape","type":"DraftIssue"},
       "id":"I3","title":"Record the Projects demo tape"},
      {"content":{"body":"","number":119,"repository":"td72/vig","title":"Projects view","type":"Issue","url":"https://github.com/td72/vig/issues/119"},
       "id":"I4","status":"Todo","title":"Projects view","priority":"P2","estimate":2},
      {"content":null,"id":"I5","status":"Blocked","title":"redacted"}
    ],"totalCount":7}"###;

    pub(crate) fn board() -> Board {
        let fields: FieldList = serde_json::from_str(FIELDS_JSON).unwrap();
        let items: ItemList = serde_json::from_str(ITEMS_JSON).unwrap();
        Board {
            number: 2,
            fields: fields.fields,
            items: items.items,
            total_count: items.total_count,
        }
    }

    #[test]
    fn parses_project_list_json() {
        let json = r#"{"projects":[{"closed":false,"fields":{"totalCount":13},"id":"PVT_1","items":{"totalCount":8},"number":2,"owner":{"login":"td72","type":"User"},"public":false,"readme":"","shortDescription":"","title":"vig demo board","url":"https://github.com/users/td72/projects/2"},{"closed":false,"fields":{"totalCount":13},"id":"PVT_2","items":{"totalCount":6},"number":1,"owner":{"login":"td72","type":"User"},"public":false,"readme":"","shortDescription":"","title":"life","url":"https://github.com/users/td72/projects/1"}],"totalCount":2}"#;
        let list: ProjectList = serde_json::from_str(json).unwrap();
        assert_eq!(list.projects.len(), 2);
        assert_eq!(list.projects[0].number, 2);
        assert_eq!(list.projects[0].title, "vig demo board");
        assert_eq!(list.projects[0].items.total_count, 8);
        assert_eq!(list.projects[0].owner.login, "td72");
        assert!(list.projects[0].updated_at.is_none());
        // Linked projects float to the top and are flagged.
        let ordered = order_projects(list.projects, &[1]);
        assert_eq!(ordered[0].number, 1);
        assert!(ordered[0].linked);
        assert!(!ordered[1].linked);
    }

    #[test]
    fn parses_repo_info_with_and_without_linked_projects() {
        let json = r#"{"name":"vig","owner":{"id":"X","login":"td72"},"projectsV2":{"Nodes":[{"id":"PVT_1","title":"vig demo board","number":2,"closed":false}]}}"#;
        let info: RepoInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.owner.login, "td72");
        assert_eq!(info.linked_numbers(), [2]);
        let info: RepoInfo = serde_json::from_str(r#"{"owner":{"login":"o"}}"#).unwrap();
        assert!(info.linked_numbers().is_empty());
    }

    #[test]
    fn parses_field_list_and_derives_item_keys() {
        let list: FieldList = serde_json::from_str(FIELDS_JSON).unwrap();
        assert_eq!(list.fields.len(), 9);
        let status = list.fields.iter().find(|f| f.is_status()).unwrap();
        let names: Vec<&str> = status.options.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Todo", "In Progress", "Done"]);
        assert_eq!(list.fields[4].item_key(), "linked pull requests");
        assert_eq!(list.fields[8].item_key(), "start date");
        assert_eq!(item_key("Priority"), "priority");
        assert_eq!(item_key(""), "");
        let table: Vec<&str> = table_fields(&list.fields)
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(
            table,
            ["Status", "Priority", "Estimate", "Iteration", "Start date"]
        );
    }

    #[test]
    fn parses_item_list_with_issue_pr_draft_and_redacted_items() {
        let list: ItemList = serde_json::from_str(ITEMS_JSON).unwrap();
        assert_eq!(list.total_count, 7);
        assert_eq!(list.items.len(), 5);
        let issue = &list.items[0];
        assert_eq!(issue.kind(), ItemKind::Issue);
        assert_eq!(issue.number(), Some(114));
        assert_eq!(issue.repository(), Some("td72/vig"));
        assert_eq!(issue.status.as_deref(), Some("Done"));
        assert_eq!(issue.assignees(), ["td72"]);
        assert_eq!(issue.field_text("estimate").as_deref(), Some("3"));
        assert_eq!(issue.field_text("priority").as_deref(), Some("P1"));
        assert_eq!(issue.field_text("labels").as_deref(), Some("enhancement"));
        assert!(issue.field_text("iteration").is_none());
        let pr = &list.items[1];
        assert_eq!(pr.kind(), ItemKind::PullRequest);
        assert_eq!(pr.field_text("estimate").as_deref(), Some("1.5"));
        assert_eq!(
            pr.field_text("iteration").as_deref(),
            Some("Sprint 3 (2026-08-24)")
        );
        assert_eq!(pr.field_text("start date").as_deref(), Some("2026-08-25"));
        assert!(pr.assignees().is_empty());
        let draft = &list.items[2];
        assert_eq!(draft.kind(), ItemKind::Draft);
        assert_eq!(draft.number(), None);
        assert!(draft.status.is_none());
        assert_eq!(draft.body(), "Draft item used by the vig demo.");
        assert_eq!(draft.title(), "Record the Projects demo tape");
        let redacted = &list.items[4];
        assert_eq!(redacted.kind(), ItemKind::Other);
        assert!(redacted.url().is_none());
    }

    #[test]
    fn field_values_follow_field_order_with_status_first() {
        let board = board();
        let values = board.items[1].field_values(&board.fields);
        let keys: Vec<&str> = values.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "Status",
                "Estimate",
                "Iteration",
                "Start date",
                "Repository"
            ]
        );
        assert_eq!(values[0].1, "In Progress");
        // A field the project no longer lists is still shown (capitalized key).
        let mut item = board.items[0].clone();
        item.fields
            .insert("legacy".into(), serde_json::Value::String("x".into()));
        let values = item.field_values(&board.fields);
        assert!(values.iter().any(|(k, v)| k == "Legacy" && v == "x"));
    }

    #[test]
    fn columns_follow_option_order_then_unknown_then_no_status() {
        let board = board();
        let cols = board.columns();
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Todo", "In Progress", "Done", "Blocked", NO_STATUS]);
        assert_eq!(cols[0].items, [3]);
        assert_eq!(cols[1].items, [1]);
        assert_eq!(cols[2].items, [0]);
        assert_eq!(cols[3].items, [4]);
        assert_eq!(cols[4].items, [2]);
        assert!(board.truncated());
        // Without a Status field every item is "No status".
        let mut plain = board.clone();
        plain.fields.retain(|f| !f.is_status());
        for item in &mut plain.items {
            item.status = None;
        }
        let cols = plain.columns();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, NO_STATUS);
        assert_eq!(cols[0].items.len(), 5);
        // No column for "No status" when every item has one.
        let mut full = board.clone();
        full.items.retain(|i| i.status.is_some());
        assert!(full.columns().iter().all(|c| c.name != NO_STATUS));
    }

    #[test]
    fn table_columns_and_sorting() {
        let board = board();
        let cols = table_columns(&board.fields);
        let headers: Vec<&str> = cols.iter().map(TableColumn::header).collect();
        assert_eq!(
            headers,
            [
                "#",
                "Title",
                "Assignees",
                "Status",
                "Priority",
                "Estimate",
                "Iteration",
                "Start date"
            ]
        );
        // Numbers sort numerically, drafts (no number) last.
        let by_number = sort_items(&board.items, &cols[0], &board);
        assert_eq!(by_number, [0, 3, 1, 2, 4]);
        // Estimate: 1.5 < 2 < 3, then the empty cells.
        let est = cols.iter().find(|c| c.header() == "Estimate").unwrap();
        let by_estimate = sort_items(&board.items, est, &board);
        assert_eq!(&by_estimate[..3], &[1, 3, 0][..]);
        // Status: option order, unknown statuses after, empty last.
        let status = cols.iter().find(|c| c.header() == "Status").unwrap();
        let by_status = sort_items(&board.items, status, &board);
        assert_eq!(by_status, [3, 1, 0, 4, 2]);
        // Titles: case-insensitive.
        let by_title = sort_items(&board.items, &cols[1], &board);
        assert_eq!(by_title[0], 0, "Config… sorts first");
    }

    #[test]
    fn value_text_covers_every_gh_shape() {
        use serde_json::json;
        assert_eq!(value_text(&json!("Todo")).as_deref(), Some("Todo"));
        assert_eq!(value_text(&json!(3)).as_deref(), Some("3"));
        assert_eq!(value_text(&json!(2.5)).as_deref(), Some("2.5"));
        assert_eq!(value_text(&json!(null)), None);
        assert_eq!(value_text(&json!([])), None);
        assert_eq!(value_text(&json!(["a", "b"])).as_deref(), Some("a, b"));
        assert_eq!(
            value_text(&json!({"title":"v1.0","description":"","dueOn":"2026-09-01"})).as_deref(),
            Some("v1.0")
        );
        assert_eq!(
            value_text(&json!({"title":"Sprint 1","startDate":"2026-08-01","duration":7}))
                .as_deref(),
            Some("Sprint 1 (2026-08-01)")
        );
    }
}
