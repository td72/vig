use crate::core::config::constraint::parse_constraint;
use crate::core::config::keymap_builder::KeymapEntry;
use crate::core::layout::{LayoutNode, PageLayoutConfig, SlotRule, SplitDirection};
use anyhow::{anyhow, Context, Result};
use kdl::{KdlDocument, KdlNode};
use std::collections::HashMap;

static DEFAULT_KDL: &str = include_str!("../../../assets/default.kdl");

// ── Public output types ───────────────────────────────────────────────────────

/// Parsed config for a single page.
pub struct LoadedPageConfig {
    /// Page name (e.g. `"git"`, `"github"`).
    pub name: String,
    pub layout: PageLayoutConfig,
    /// Pane keymaps keyed by pane name (e.g. `"file_tree"`, `"view"`, …).
    pub pane_keys: HashMap<String, Vec<KeymapEntry>>,
    /// Pane names and their auto-assigned IDs (declaration order, layout panes only).
    pub pane_ids: Vec<(String, usize)>,
    /// select→detail instance bindings declared in the KDL.
    pub bindings: Vec<(String, String)>,
}

impl LoadedPageConfig {
    /// Resolve a pane name to its numeric ID.
    pub fn resolve_id(&self, name: &str) -> Option<usize> {
        self.pane_ids
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, id)| *id)
    }

    /// Resolve a pane name to its numeric ID, panicking if not found.
    /// Use this for pane names known statically by the page (e.g. `"file_tree"`).
    pub fn resolve_id_expect(&self, name: &str) -> usize {
        self.resolve_id(name)
            .unwrap_or_else(|| panic!("pane {name:?} not found in {:?} page config", self.name))
    }

    /// Build a `select_id → detail_id` map from the KDL `bind` declarations.
    /// Panics if any pane name in the bindings does not exist in this config.
    pub fn resolve_select_bindings(&self) -> HashMap<usize, usize> {
        self.bindings
            .iter()
            .map(|(sel, det)| {
                let s = self
                    .resolve_id(sel)
                    .unwrap_or_else(|| panic!("bind select={sel:?}: pane not found in config"));
                let d = self
                    .resolve_id(det)
                    .unwrap_or_else(|| panic!("bind detail={det:?}: pane not found in config"));
                (s, d)
            })
            .collect()
    }
}

// ── App-block parser ──────────────────────────────────────────────────────────

fn parse_app_block(doc: &KdlDocument) -> Result<Vec<(String, String)>> {
    let mut entries = Vec::new();
    for node in doc.nodes() {
        let key_str = node.name().value().to_string();
        let action_str = node
            .get(0usize)
            .and_then(|v| v.as_string())
            .ok_or_else(|| anyhow!("app entry {key_str:?} missing action argument"))?
            .to_string();
        entries.push((key_str, action_str));
    }
    Ok(entries)
}

// ── Public loaders ────────────────────────────────────────────────────────────

/// Parse the git page from the embedded default config.
/// Pane IDs are auto-assigned in declaration order (layout panes only).
pub fn load_git_page_config() -> Result<LoadedPageConfig> {
    let doc: KdlDocument = DEFAULT_KDL.parse().context("KDL parse error")?;
    load_page_from_doc(&doc, "git")
}

/// Parse the github page from the embedded default config.
/// Pane IDs are auto-assigned in declaration order (layout panes only).
pub fn load_github_page_config() -> Result<LoadedPageConfig> {
    let doc: KdlDocument = DEFAULT_KDL.parse().context("KDL parse error")?;
    load_page_from_doc(&doc, "github")
}

/// Parse app-level key entries from the embedded default config.
/// Returns `(key_str, action_str)` pairs.
pub fn load_app_entries() -> Vec<(String, String)> {
    let doc: KdlDocument = DEFAULT_KDL.parse().expect("default.kdl is always valid");
    doc.nodes()
        .iter()
        .find(|n| n.name().value() == "app")
        .and_then(|n| n.children())
        .map(|c| parse_app_block(c).expect("default.kdl app block is always valid"))
        .unwrap_or_default()
}

fn load_page_from_doc(doc: &KdlDocument, page_name: &str) -> Result<LoadedPageConfig> {
    let page_node = doc
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "page")
        .find(|n| n.get(0usize).and_then(|v| v.as_string()) == Some(page_name))
        .ok_or_else(|| anyhow!("page {page_name:?} not found in default.kdl"))?;

    let children = page_node
        .children()
        .ok_or_else(|| anyhow!("page {page_name:?} has no children block"))?;

    // Layout
    let layout_doc = children
        .nodes()
        .iter()
        .find(|n| n.name().value() == "layout")
        .and_then(|n| n.children())
        .ok_or_else(|| anyhow!("page {page_name:?} missing layout block"))?;

    // Build pane_ids: pre-scan layout for pane names, then assign IDs in pane block order.
    let layout_root = layout_doc
        .nodes()
        .first()
        .ok_or_else(|| anyhow!("page {page_name:?} layout is empty"))?;
    let layout_pane_names = collect_layout_pane_names(layout_root);
    let pane_ids = build_pane_ids(children, &layout_pane_names);

    // Build name_map from pane_ids for layout / tab / slot parsing.
    let name_map: HashMap<&str, usize> = pane_ids
        .iter()
        .map(|(name, id)| (name.as_str(), *id))
        .collect();

    // Tab panes
    let tab_panes = parse_tabs(children, &name_map, page_name)?;

    // Parse layout tree + slot rules together
    let mut slot_id_counter: usize = 0;
    let mut slot_name_to_id: HashMap<String, usize> = HashMap::new();
    let mut slot_rules: Vec<SlotRule> = Vec::new();

    let root = parse_layout_root(
        layout_doc,
        &name_map,
        &mut slot_id_counter,
        &mut slot_name_to_id,
        &mut slot_rules,
        page_name,
    )?;

    // Pane keymaps
    let pane_keys = parse_all_pane_keys(children, page_name)?;

    // select→detail bindings
    let bindings = parse_bindings(children, &name_map, page_name)?;

    Ok(LoadedPageConfig {
        name: page_name.to_string(),
        layout: PageLayoutConfig {
            tree: root,
            tab_panes,
            slot_rules,
        },
        pane_keys,
        pane_ids,
        bindings,
    })
}

// ── Pane ID builder ───────────────────────────────────────────────────────────

/// Collect all pane names referenced in a layout node
/// (from `place`, `slot then=`/`default=`, and `triggers` children),
/// deduplicated while preserving first-occurrence order.
fn collect_layout_pane_names(node: &KdlNode) -> Vec<String> {
    let mut names = Vec::new();
    collect_layout_pane_names_into(node, &mut names);
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

fn collect_layout_pane_names_into(node: &KdlNode, names: &mut Vec<String>) {
    match node.name().value() {
        "place" => {
            if let Some(name) = node.get(0usize).and_then(|v| v.as_string()) {
                names.push(name.to_string());
            }
        }
        "slot" => {
            if let Some(name) = node.get("then").and_then(|v| v.as_string()) {
                names.push(name.to_string());
            }
            if let Some(name) = node.get("default").and_then(|v| v.as_string()) {
                names.push(name.to_string());
            }
            if let Some(children) = node.children() {
                for child in children.nodes() {
                    if child.name().value() == "triggers" {
                        for entry in child.entries().iter().filter(|e| e.name().is_none()) {
                            if let Some(name) = entry.value().as_string() {
                                names.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
        "split" => {
            if let Some(children) = node.children() {
                for child in children.nodes() {
                    collect_layout_pane_names_into(child, names);
                }
            }
        }
        _ => {}
    }
}

/// Assign sequential IDs to panes that appear in the layout,
/// in the order they appear as `pane "<name>"` blocks in the page.
fn build_pane_ids(page_children: &KdlDocument, layout_names: &[String]) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    let mut id = 0usize;
    for node in page_children.nodes() {
        if node.name().value() == "pane" {
            if let Some(name) = node.get(0usize).and_then(|v| v.as_string()) {
                if layout_names.contains(&name.to_string()) {
                    result.push((name.to_string(), id));
                    id += 1;
                }
            }
        }
    }
    result
}

// ── Bindings parser ───────────────────────────────────────────────────────────

fn parse_bindings(
    page_children: &KdlDocument,
    name_map: &HashMap<&str, usize>,
    page: &str,
) -> Result<Vec<(String, String)>> {
    page_children
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "bind")
        .map(|n| {
            let select = n
                .get("select")
                .and_then(|v| v.as_string())
                .ok_or_else(|| anyhow!("bind in page {page:?} missing select= attribute"))?
                .to_string();
            let detail = n
                .get("detail")
                .and_then(|v| v.as_string())
                .ok_or_else(|| anyhow!("bind in page {page:?} missing detail= attribute"))?
                .to_string();
            if !name_map.contains_key(select.as_str()) {
                return Err(anyhow!(
                    "bind select={select:?} in page {page:?}: pane not found"
                ));
            }
            if !name_map.contains_key(detail.as_str()) {
                return Err(anyhow!(
                    "bind detail={detail:?} in page {page:?}: pane not found"
                ));
            }
            Ok((select, detail))
        })
        .collect()
}

// ── Tab parser ─────────────────────────────────────────────────────────────────

fn parse_tabs(
    doc: &KdlDocument,
    name_map: &HashMap<&str, usize>,
    page: &str,
) -> Result<Vec<usize>> {
    let tabs_node = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "tabs")
        .ok_or_else(|| anyhow!("page {page:?} missing tabs node"))?;

    tabs_node
        .entries()
        .iter()
        .filter(|e| e.name().is_none()) // positional args only
        .map(|e| {
            let pane_name = e
                .value()
                .as_string()
                .ok_or_else(|| anyhow!("tabs entry is not a string"))?;
            name_map
                .get(pane_name)
                .copied()
                .ok_or_else(|| anyhow!("unknown pane in tabs: {pane_name:?}"))
        })
        .collect()
}

// ── Layout parser ─────────────────────────────────────────────────────────────

/// Parse the root layout node (the outermost element inside `layout { }`).
fn parse_layout_root(
    doc: &KdlDocument,
    name_map: &HashMap<&str, usize>,
    slot_id_counter: &mut usize,
    slot_name_to_id: &mut HashMap<String, usize>,
    slot_rules: &mut Vec<SlotRule>,
    page: &str,
) -> Result<LayoutNode> {
    let root_node = doc
        .nodes()
        .first()
        .ok_or_else(|| anyhow!("page {page:?} layout is empty"))?;

    parse_layout_element(
        root_node,
        name_map,
        slot_id_counter,
        slot_name_to_id,
        slot_rules,
    )
}

fn parse_layout_element(
    node: &KdlNode,
    name_map: &HashMap<&str, usize>,
    slot_id_counter: &mut usize,
    slot_name_to_id: &mut HashMap<String, usize>,
    slot_rules: &mut Vec<SlotRule>,
) -> Result<LayoutNode> {
    match node.name().value() {
        "split" => parse_split(node, name_map, slot_id_counter, slot_name_to_id, slot_rules),
        "place" => parse_place(node, name_map),
        "slot" => parse_slot(node, name_map, slot_id_counter, slot_name_to_id, slot_rules),
        other => Err(anyhow!("unknown layout element: {other:?}")),
    }
}

fn parse_split(
    node: &KdlNode,
    name_map: &HashMap<&str, usize>,
    slot_id_counter: &mut usize,
    slot_name_to_id: &mut HashMap<String, usize>,
    slot_rules: &mut Vec<SlotRule>,
) -> Result<LayoutNode> {
    let dir_str = node
        .get("direction")
        .and_then(|v| v.as_string())
        .ok_or_else(|| anyhow!("split missing direction="))?;
    let direction = match dir_str {
        "horizontal" => SplitDirection::Horizontal,
        "vertical" => SplitDirection::Vertical,
        d => return Err(anyhow!("unknown split direction: {d:?}")),
    };

    let child_doc = node
        .children()
        .ok_or_else(|| anyhow!("split has no children block"))?;

    let mut children = Vec::new();
    for child_node in child_doc.nodes() {
        let size_str = child_node
            .get("size")
            .and_then(|v| v.as_string())
            .unwrap_or("min:0");
        let constraint =
            parse_constraint(size_str).map_err(|e| anyhow!("split child size error: {e}"))?;
        let child_layout = parse_layout_element(
            child_node,
            name_map,
            slot_id_counter,
            slot_name_to_id,
            slot_rules,
        )?;
        children.push((constraint, child_layout));
    }

    Ok(LayoutNode::Split {
        direction,
        children,
    })
}

fn parse_place(node: &KdlNode, name_map: &HashMap<&str, usize>) -> Result<LayoutNode> {
    let pane_name = node
        .get(0usize)
        .and_then(|v| v.as_string())
        .ok_or_else(|| anyhow!("place missing pane name argument"))?;
    let pane_id = *name_map
        .get(pane_name)
        .ok_or_else(|| anyhow!("unknown pane in place: {pane_name:?}"))?;
    Ok(LayoutNode::Pane(pane_id))
}

fn parse_slot(
    node: &KdlNode,
    name_map: &HashMap<&str, usize>,
    slot_id_counter: &mut usize,
    slot_name_to_id: &mut HashMap<String, usize>,
    slot_rules: &mut Vec<SlotRule>,
) -> Result<LayoutNode> {
    let slot_name = node
        .get(0usize)
        .and_then(|v| v.as_string())
        .ok_or_else(|| anyhow!("slot missing name argument"))?
        .to_string();

    let slot_id = *slot_name_to_id.entry(slot_name.clone()).or_insert_with(|| {
        let id = *slot_id_counter;
        *slot_id_counter += 1;
        id
    });

    let then_name = node
        .get("then")
        .and_then(|v| v.as_string())
        .ok_or_else(|| anyhow!("slot {slot_name:?} missing then="))?;
    let default_name = node
        .get("default")
        .and_then(|v| v.as_string())
        .ok_or_else(|| anyhow!("slot {slot_name:?} missing default="))?;

    let then_pane = *name_map
        .get(then_name)
        .ok_or_else(|| anyhow!("unknown pane in slot then=: {then_name:?}"))?;
    let default_pane = *name_map
        .get(default_name)
        .ok_or_else(|| anyhow!("unknown pane in slot default=: {default_name:?}"))?;

    // Parse triggers from child block
    let trigger_panes =
        node.children()
            .map(|child_doc| {
                child_doc
                    .nodes()
                    .iter()
                    .find(|n| n.name().value() == "triggers")
                    .map(|triggers_node| {
                        triggers_node
                            .entries()
                            .iter()
                            .filter(|e| e.name().is_none())
                            .map(|e| {
                                let pane_name = e
                                    .value()
                                    .as_string()
                                    .ok_or_else(|| anyhow!("trigger entry is not a string"))?;
                                name_map.get(pane_name).copied().ok_or_else(|| {
                                    anyhow!("unknown pane in triggers: {pane_name:?}")
                                })
                            })
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()
            })
            .transpose()?
            .flatten()
            .unwrap_or_default();

    slot_rules.push(SlotRule {
        slot_id,
        trigger_panes,
        then_pane,
        default_pane,
    });

    Ok(LayoutNode::Slot(slot_id))
}

// ── Pane keys parser ──────────────────────────────────────────────────────────

fn parse_all_pane_keys(
    page_children: &KdlDocument,
    page: &str,
) -> Result<HashMap<String, Vec<KeymapEntry>>> {
    let mut result = HashMap::new();
    for node in page_children.nodes() {
        if node.name().value() == "pane" {
            let pane_name = node
                .get(0usize)
                .and_then(|v| v.as_string())
                .ok_or_else(|| anyhow!("pane node in page {page:?} missing name argument"))?
                .to_string();
            let keys = parse_pane_keys(node, &pane_name)?;
            result.insert(pane_name, keys);
        }
    }
    Ok(result)
}

fn parse_pane_keys(pane_node: &KdlNode, pane_name: &str) -> Result<Vec<KeymapEntry>> {
    let pane_children = pane_node
        .children()
        .ok_or_else(|| anyhow!("pane {pane_name:?} has no children block"))?;

    let keys_node = pane_children
        .nodes()
        .iter()
        .find(|n| n.name().value() == "keys")
        .ok_or_else(|| anyhow!("pane {pane_name:?} missing keys {{ }} block"))?;

    let keys_children = keys_node
        .children()
        .ok_or_else(|| anyhow!("pane {pane_name:?} keys block is empty"))?;

    let mut entries = Vec::new();
    for node in keys_children.nodes() {
        match node.name().value() {
            "preset" => {
                let preset_name = node
                    .get(0usize)
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| anyhow!("preset in pane {pane_name:?} missing name argument"))?
                    .to_string();
                entries.push(KeymapEntry::Preset(preset_name));
            }
            key_str => {
                let action_str = node
                    .get(0usize)
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| {
                        anyhow!("key {key_str:?} in pane {pane_name:?} missing action argument")
                    })?
                    .to_string();
                entries.push(KeymapEntry::Binding {
                    key: key_str.to_string(),
                    action: action_str,
                });
            }
        }
    }
    Ok(entries)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_git_page() {
        let cfg = load_git_page_config().unwrap();
        assert_eq!(cfg.name, "git");
    }

    #[test]
    fn load_github_page() {
        let cfg = load_github_page_config().unwrap();
        assert_eq!(cfg.name, "github");
    }

    #[test]
    fn app_entries_populated() {
        let entries = load_app_entries();
        assert!(!entries.is_empty());
        let has_quit = entries.iter().any(|(_, a)| a == "Quit");
        assert!(has_quit, "app entries should contain Quit");
    }

    #[test]
    fn git_pane_keys_present() {
        let cfg = load_git_page_config().unwrap();
        for name in &[
            "view",
            "file_tree",
            "branch_list",
            "git_log",
            "reflog",
            "diff_view",
        ] {
            assert!(
                cfg.pane_keys.contains_key(*name),
                "missing pane keys for {name}"
            );
        }
    }

    #[test]
    fn github_pane_keys_present() {
        let cfg = load_github_page_config().unwrap();
        for name in &["view", "issue_list", "pr_list", "issue_detail", "pr_detail"] {
            assert!(
                cfg.pane_keys.contains_key(*name),
                "missing pane keys for {name}"
            );
        }
    }

    #[test]
    fn git_pane_ids_correct() {
        let cfg = load_git_page_config().unwrap();
        // IDs assigned in pane block declaration order (excluding "view")
        let expected = [
            ("file_tree", 0usize),
            ("branch_list", 1),
            ("git_log", 2),
            ("reflog", 3),
            ("diff_view", 4),
        ];
        for (name, id) in &expected {
            assert_eq!(
                cfg.resolve_id(name),
                Some(*id),
                "pane {name:?} should have id {id}"
            );
        }
    }

    #[test]
    fn github_pane_ids_correct() {
        let cfg = load_github_page_config().unwrap();
        let expected = [
            ("issue_list", 0usize),
            ("pr_list", 1),
            ("issue_detail", 2),
            ("pr_detail", 3),
        ];
        for (name, id) in &expected {
            assert_eq!(
                cfg.resolve_id(name),
                Some(*id),
                "pane {name:?} should have id {id}"
            );
        }
    }

    #[test]
    fn git_bindings_correct() {
        let cfg = load_git_page_config().unwrap();
        assert!(
            cfg.bindings
                .contains(&("file_tree".to_string(), "diff_view".to_string())),
            "missing bind file_tree→diff_view"
        );
        assert!(
            cfg.bindings
                .contains(&("branch_list".to_string(), "git_log".to_string())),
            "missing bind branch_list→git_log"
        );
    }

    #[test]
    fn github_bindings_correct() {
        let cfg = load_github_page_config().unwrap();
        assert!(
            cfg.bindings
                .contains(&("issue_list".to_string(), "issue_detail".to_string())),
            "missing bind issue_list→issue_detail"
        );
        assert!(
            cfg.bindings
                .contains(&("pr_list".to_string(), "pr_detail".to_string())),
            "missing bind pr_list→pr_detail"
        );
    }

    #[test]
    fn github_bindings_distinct_instances() {
        // Verify that issue_detail and pr_detail are different instances (different IDs)
        let cfg = load_github_page_config().unwrap();
        let issue_detail_id = cfg.resolve_id("issue_detail").unwrap();
        let pr_detail_id = cfg.resolve_id("pr_detail").unwrap();
        assert_ne!(
            issue_detail_id, pr_detail_id,
            "issue_detail and pr_detail must be distinct instances"
        );
        // Verify they are bound to different select panes
        let issue_binding = cfg
            .bindings
            .iter()
            .find(|(_, d)| d == "issue_detail")
            .expect("issue_detail binding missing");
        let pr_binding = cfg
            .bindings
            .iter()
            .find(|(_, d)| d == "pr_detail")
            .expect("pr_detail binding missing");
        assert_ne!(
            issue_binding.0, pr_binding.0,
            "issue_detail and pr_detail must be bound to different select panes"
        );
    }
}
