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

/// `gh repo view --json nameWithOwner,owner,projectsV2`: the repository
/// vig runs in, its owner and the projects linked to it. The linked
/// projects are the page's only data source.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepoInfo {
    /// `owner/repo`, to tell cards of other repositories apart.
    #[serde(rename = "nameWithOwner", default)]
    pub name_with_owner: String,
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

/// One node of `projectsV2`: `gh` emits `id`, `title`, `number`,
/// `resourcePath`, `closed` and `url`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LinkedProject {
    pub number: u64,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(rename = "resourcePath", default)]
    pub resource_path: String,
    #[serde(default)]
    pub closed: bool,
}

impl LinkedProject {
    /// The login owning the project, read off `resourcePath`
    /// (`/users/<login>/projects/<n>` or `/orgs/<login>/projects/<n>`):
    /// `gh project` commands take it as `--owner`.
    pub fn owner_login(&self) -> Option<(&str, &str)> {
        let mut parts = self.resource_path.trim_start_matches('/').split('/');
        let kind = match parts.next()? {
            "users" => "User",
            "orgs" => "Organization",
            _ => return None,
        };
        let login = parts.next().filter(|l| !l.is_empty())?;
        Some((login, kind))
    }

    /// The [`Project`] the page works with; `repo_owner` is the fallback
    /// owner when `resourcePath` is missing.
    pub fn to_project(&self, repo_owner: &str) -> Project {
        let (login, kind) = self
            .owner_login()
            .map(|(l, k)| (l.to_string(), k.to_string()))
            .unwrap_or_else(|| (repo_owner.to_string(), String::new()));
        Project {
            number: self.number,
            title: self.title.clone(),
            id: self.id.clone(),
            url: self.url.clone(),
            closed: self.closed,
            items: Count::default(),
            owner: ProjectOwner { login, kind },
            linked: true,
        }
    }
}

impl RepoInfo {
    /// The open projects linked to the repository, in GitHub's order.
    pub fn linked_projects(&self) -> Vec<Project> {
        self.projects_v2
            .nodes
            .iter()
            .filter(|p| !p.closed)
            .map(|p| p.to_project(&self.owner.login))
            .collect()
    }
}

// === Projects ===

/// A project the page can show: one of the repository's linked projects.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Project {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub closed: bool,
    /// Item count, known once the board has been fetched (0 until then).
    #[serde(default)]
    pub items: Count,
    #[serde(default)]
    pub owner: ProjectOwner,
    /// Linked to the repository vig runs in (marked in the list pane).
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

/// What the page keeps on disk between runs: the linked projects, so a
/// board can show up before `gh repo view` answers.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProjectListCache {
    /// `owner/repo` the list belongs to.
    #[serde(default)]
    pub repo: String,
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

// === Saved views ===

/// Layout of a saved project view (`ProjectV2View.layout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum ViewLayout {
    Table,
    Board,
    Roadmap,
}

impl ViewLayout {
    /// From the GraphQL enum (`TABLE_LAYOUT`, `BOARD_LAYOUT`, `ROADMAP_LAYOUT`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "TABLE_LAYOUT" => Some(Self::Table),
            "BOARD_LAYOUT" => Some(Self::Board),
            "ROADMAP_LAYOUT" => Some(Self::Roadmap),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Board => "board",
            Self::Roadmap => "roadmap",
        }
    }
}

/// One saved view of a project (`ProjectV2.views`): the layout people set
/// up on GitHub, with the field names driving its columns, grouping and
/// sorting. Field references are stored by name and resolved against
/// [`Board::fields`] at render time.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectView {
    pub number: u64,
    #[serde(default)]
    pub name: String,
    pub layout: ViewLayout,
    /// The view's filter expression, verbatim (`status:Todo -label:bug`).
    #[serde(default)]
    pub filter: Option<String>,
    /// Horizontal grouping (table group rows / board swimlanes).
    #[serde(default)]
    pub group_by: Vec<String>,
    /// The field whose options become the board columns.
    #[serde(default)]
    pub vertical_group_by: Vec<String>,
    /// Sort keys with their direction (`ASC` / `DESC`).
    #[serde(default)]
    pub sort_by: Vec<ViewSort>,
    /// Visible fields in the view's column order.
    #[serde(default)]
    pub visible_fields: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ViewSort {
    pub field: String,
    /// `true` for descending (`DESC`).
    #[serde(default)]
    pub desc: bool,
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
    /// The project's saved views, in GitHub's order (empty when the fetch
    /// failed or the project has none — the fixed Status kanban is shown).
    #[serde(default)]
    pub views: Vec<ProjectView>,
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
/// last (whatever the direction); ties keep the board order. `Status`
/// sorts by option order. `desc` flips the order of the non-empty cells.
pub fn sort_items_dir(
    items: &[ProjectItem],
    column: &TableColumn,
    board: &Board,
    desc: bool,
) -> Vec<usize> {
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
    let flip = |o: std::cmp::Ordering| if desc { o.reverse() } else { o };
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
                return flip(ra.cmp(&rb));
            }
        }
        flip(if numeric {
            let na: f64 = ca.trim_start_matches('#').parse().unwrap_or(0.0);
            let nb: f64 = cb.trim_start_matches('#').parse().unwrap_or(0.0);
            na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            // Compare lowercased char streams without allocating per comparison.
            ca.chars()
                .flat_map(char::to_lowercase)
                .cmp(cb.chars().flat_map(char::to_lowercase))
        })
    });
    order
}

/// Table columns for a saved Table view: `#`, then the view's visible
/// fields in its order. Fields vig cannot render as a cell (the built-in
/// ones other than `Title` / `Assignees`, and names the project no longer
/// lists) are skipped; a view without a usable `Title` still gets one so
/// every row has a label.
pub fn view_table_columns(view: &ProjectView, fields: &[ProjectField]) -> Vec<TableColumn> {
    let mut cols = vec![TableColumn::Number];
    for name in &view.visible_fields {
        match name.as_str() {
            "Title" => cols.push(TableColumn::Title),
            "Assignees" => cols.push(TableColumn::Assignees),
            n if !BUILTIN_FIELDS.contains(&n) && fields.iter().any(|f| f.name == n) => {
                cols.push(TableColumn::Field {
                    name: n.to_string(),
                    key: item_key(n),
                })
            }
            _ => {}
        }
    }
    if !cols.contains(&TableColumn::Title) {
        cols.insert(1, TableColumn::Title);
    }
    cols
}

/// Bucket `order` (already sorted item indices) into groups by `field`'s
/// value: single-select options in GitHub's order first, then values the
/// options do not list (in first-seen order), then `No <field>` for items
/// without a value. Groups keep `order`'s ordering inside.
pub fn group_rows(
    board: &Board,
    field: &ProjectField,
    order: &[usize],
) -> Vec<(String, Vec<usize>)> {
    let key = field.item_key();
    let mut groups: Vec<(String, Vec<usize>)> = field
        .options
        .iter()
        .map(|o| (o.name.clone(), Vec::new()))
        .collect();
    let mut none: Vec<usize> = Vec::new();
    for &idx in order {
        let Some(item) = board.items.get(idx) else {
            continue;
        };
        match item.field_text(&key).filter(|v| !v.is_empty()) {
            Some(v) => match groups.iter_mut().find(|(name, _)| *name == v) {
                Some((_, items)) => items.push(idx),
                None => groups.push((v, vec![idx])),
            },
            None => none.push(idx),
        }
    }
    groups.retain(|(_, items)| !items.is_empty());
    if !none.is_empty() {
        groups.push((format!("No {}", field.name.to_lowercase()), none));
    }
    groups
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

    #[test]
    fn view_layout_parses_graphql_enum() {
        assert_eq!(ViewLayout::parse("TABLE_LAYOUT"), Some(ViewLayout::Table));
        assert_eq!(ViewLayout::parse("BOARD_LAYOUT"), Some(ViewLayout::Board));
        assert_eq!(
            ViewLayout::parse("ROADMAP_LAYOUT"),
            Some(ViewLayout::Roadmap)
        );
        assert_eq!(ViewLayout::parse("SOMETHING_NEW"), None);
        assert_eq!(ViewLayout::Roadmap.label(), "roadmap");
    }

    #[test]
    fn board_without_views_deserializes_from_old_cache() {
        // A board-<n>.json written before views existed loads with none.
        let b: Board =
            serde_json::from_str(r#"{"number":7,"fields":[],"items":[],"total_count":0}"#).unwrap();
        assert!(b.views.is_empty());
        // And one with views round-trips.
        let mut b = board();
        b.views.push(ProjectView {
            number: 1,
            name: "Sprint".into(),
            layout: ViewLayout::Board,
            filter: Some("status:Todo".into()),
            group_by: vec![],
            vertical_group_by: vec!["Status".into()],
            sort_by: vec![ViewSort {
                field: "Priority".into(),
                desc: true,
            }],
            visible_fields: vec!["Title".into(), "Status".into()],
        });
        let json = serde_json::to_string(&b).unwrap();
        let back: Board = serde_json::from_str(&json).unwrap();
        assert_eq!(back.views.len(), 1);
        assert_eq!(back.views[0].name, "Sprint");
        assert_eq!(back.views[0].layout, ViewLayout::Board);
        assert!(back.views[0].sort_by[0].desc);
    }

    #[test]
    fn view_table_columns_follow_the_view() {
        let b = board();
        let mut v = ProjectView {
            number: 1,
            name: "V".into(),
            layout: ViewLayout::Table,
            filter: None,
            group_by: vec![],
            vertical_group_by: vec![],
            sort_by: vec![],
            visible_fields: vec![
                "Priority".into(),
                "Title".into(),
                "Linked pull requests".into(), // built-in: skipped
                "Ghost".into(),                // unknown: skipped
                "Estimate".into(),
                "Assignees".into(),
            ],
        };
        let cols = view_table_columns(&v, &b.fields);
        let headers: Vec<&str> = cols.iter().map(TableColumn::header).collect();
        assert_eq!(
            headers,
            vec!["#", "Priority", "Title", "Estimate", "Assignees"]
        );
        // A view without Title still gets one.
        v.visible_fields = vec!["Priority".into()];
        let cols = view_table_columns(&v, &b.fields);
        let headers: Vec<&str> = cols.iter().map(TableColumn::header).collect();
        assert_eq!(headers, vec!["#", "Title", "Priority"]);
    }

    #[test]
    fn sort_items_dir_desc_keeps_empty_cells_last() {
        let b = board();
        let cols = table_columns(&b.fields);
        let est = cols.iter().find(|c| c.header() == "Estimate").unwrap();
        let asc = sort_items_dir(&b.items, est, &b, false);
        let desc = sort_items_dir(&b.items, est, &b, true);
        let cell = |o: &[usize], k: usize| est.cell(&b.items[o[k]]);
        // Ascending: smallest first; descending: largest first.
        assert!(cell(&asc, 0) <= cell(&asc, 1));
        assert!(cell(&desc, 0) >= cell(&desc, 1));
        // Items without an estimate stay at the end in both directions.
        assert_eq!(cell(&asc, asc.len() - 1), "");
        assert_eq!(cell(&desc, desc.len() - 1), "");
    }

    #[test]
    fn group_rows_use_option_order_and_no_group_last() {
        let b = board();
        let status = b.status_field().unwrap().clone();
        let order: Vec<usize> = (0..b.items.len()).collect();
        let groups = group_rows(&b, &status, &order);
        let labels: Vec<&str> = groups.iter().map(|(l, _)| l.as_str()).collect();
        // Options in GitHub's order; "Blocked" is a value the options no
        // longer list; unset items go last.
        assert_eq!(
            labels,
            vec!["Todo", "In Progress", "Done", "Blocked", "No status"]
        );
        let total: usize = groups.iter().map(|(_, i)| i.len()).sum();
        assert_eq!(total, b.items.len());
    }

    pub(crate) fn board() -> Board {
        let fields: FieldList = serde_json::from_str(FIELDS_JSON).unwrap();
        let items: ItemList = serde_json::from_str(ITEMS_JSON).unwrap();
        Board {
            number: 2,
            fields: fields.fields,
            items: items.items,
            total_count: items.total_count,
            views: vec![],
        }
    }

    /// `gh repo view --json nameWithOwner,owner,projectsV2` for a repository
    /// with a user project, an organization project and a closed one linked.
    pub(crate) const REPO_JSON: &str = r#"{"nameWithOwner":"td72/vig","owner":{"id":"X","login":"td72"},"projectsV2":{"Nodes":[
      {"id":"PVT_1","title":"vig demo board","number":2,"resourcePath":"/users/td72/projects/2","closed":false,"url":"https://github.com/users/td72/projects/2"},
      {"id":"PVT_2","title":"Roadmap","number":7,"resourcePath":"/orgs/acme/projects/7","closed":false,"url":"https://github.com/orgs/acme/projects/7"},
      {"id":"PVT_3","title":"Old","number":1,"resourcePath":"/users/td72/projects/1","closed":true,"url":"https://github.com/users/td72/projects/1"}
    ]}}"#;

    pub(crate) fn repo_info() -> RepoInfo {
        serde_json::from_str(REPO_JSON).unwrap()
    }

    #[test]
    fn parses_repo_info_into_the_linked_projects() {
        let info = repo_info();
        assert_eq!(info.name_with_owner, "td72/vig");
        assert_eq!(info.owner.login, "td72");
        let numbers: Vec<u64> = info.projects_v2.nodes.iter().map(|p| p.number).collect();
        assert_eq!(numbers, [2, 7, 1]);
        // Closed projects are dropped; the owner comes from resourcePath.
        let linked = info.linked_projects();
        assert_eq!(linked.len(), 2);
        assert_eq!(linked[0].number, 2);
        assert_eq!(linked[0].title, "vig demo board");
        assert_eq!(linked[0].url, "https://github.com/users/td72/projects/2");
        assert_eq!(linked[0].owner.login, "td72");
        assert_eq!(linked[0].owner.kind, "User");
        assert!(linked[0].linked);
        assert_eq!(
            linked[0].items.total_count, 0,
            "unknown until the board loads"
        );
        assert_eq!(linked[1].number, 7);
        assert_eq!(linked[1].owner.login, "acme");
        assert_eq!(linked[1].owner.kind, "Organization");
        // No resourcePath: fall back to the repository owner.
        let bare = LinkedProject {
            number: 3,
            ..Default::default()
        };
        assert_eq!(bare.owner_login(), None);
        assert_eq!(bare.to_project("td72").owner.login, "td72");
        // Older gh output without the field, and a repository without links.
        let info: RepoInfo = serde_json::from_str(r#"{"owner":{"login":"o"}}"#).unwrap();
        assert!(info.projects_v2.nodes.is_empty());
        assert!(info.linked_projects().is_empty());
        let info: RepoInfo =
            serde_json::from_str(r#"{"owner":{"login":"o"},"projectsV2":{"Nodes":[]}}"#).unwrap();
        assert!(info.linked_projects().is_empty());
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
        let by_number = sort_items_dir(&board.items, &cols[0], &board, false);
        assert_eq!(by_number, [0, 3, 1, 2, 4]);
        // Estimate: 1.5 < 2 < 3, then the empty cells.
        let est = cols.iter().find(|c| c.header() == "Estimate").unwrap();
        let by_estimate = sort_items_dir(&board.items, est, &board, false);
        assert_eq!(&by_estimate[..3], &[1, 3, 0][..]);
        // Status: option order, unknown statuses after, empty last.
        let status = cols.iter().find(|c| c.header() == "Status").unwrap();
        let by_status = sort_items_dir(&board.items, status, &board, false);
        assert_eq!(by_status, [3, 1, 0, 4, 2]);
        // Titles: case-insensitive.
        let by_title = sort_items_dir(&board.items, &cols[1], &board, false);
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
