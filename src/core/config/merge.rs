//! Overlay a user KDL document on top of the built-in default document.
//!
//! Merge rules (see `docs/config.md`):
//! - `theme "<name>"`, `icons "<mode>"`, `image-preview "<mode>"`,
//!   `procs-refresh-interval "<duration>"`, `procs-history "<n>"`,
//!   `github-poll-interval "<duration>"`, `projects-board <title-or-number>`,
//!   `pages "<name>" ...` — replaced.
//! - `app { }` — merged per key; a user entry replaces the default entry with the same key.
//! - `page "x"` — must exist in the defaults.
//!   - `layout { }`, `tabs`, `bind` — replaced wholesale when present in the user page.
//!   - `pane "y" { keys { } }` — the pane must exist in the defaults; keys are merged per key.
//!     `preset` nodes are always appended.
//! - Anything else is an error, so typos fail fast instead of being silently ignored.

use anyhow::{anyhow, Result};
use kdl::{KdlDocument, KdlNode};

fn arg0(node: &KdlNode) -> Option<&str> {
    node.get(0usize).and_then(|v| v.as_string())
}

fn find_mut(doc: &mut KdlDocument, pred: impl Fn(&KdlNode) -> bool) -> Option<&mut KdlNode> {
    doc.nodes_mut().iter_mut().find(|n| pred(n))
}

/// Merge `user` into `default` in place.
pub fn merge_user_config(default: &mut KdlDocument, user: &KdlDocument) -> Result<()> {
    for unode in user.nodes() {
        match unode.name().value() {
            "theme"
            | "icons"
            | "image-preview"
            | "procs-refresh-interval"
            | "procs-history"
            | "github-poll-interval"
            | "projects-board"
            | "pages" => replace_single(default, unode),
            "app" => merge_app(default, unode)?,
            "page" => merge_page(default, unode)?,
            other => {
                return Err(anyhow!(
                "unknown top-level block {other:?} (expected `theme`, `icons`, `image-preview`, \
                 `procs-refresh-interval`, `procs-history`, `github-poll-interval`, \
                 `projects-board`, `pages`, `app`, or `page`)"
            ))
            }
        }
    }
    Ok(())
}

/// Replace the (single) node with the same name, or append it.
fn replace_single(target: &mut KdlDocument, node: &KdlNode) {
    let name = node.name().value();
    let nodes = target.nodes_mut();
    match nodes.iter_mut().find(|n| n.name().value() == name) {
        Some(existing) => *existing = node.clone(),
        None => nodes.push(node.clone()),
    }
}

fn merge_app(default: &mut KdlDocument, unode: &KdlNode) -> Result<()> {
    let Some(uchildren) = unode.children() else {
        return Ok(());
    };
    let dnode = find_mut(default, |n| n.name().value() == "app")
        .ok_or_else(|| anyhow!("built-in config has no app block"))?;
    merge_key_entries(dnode.ensure_children(), uchildren);
    Ok(())
}

/// Replace-or-append each source node into `target`, matching on the node name
/// (the key string). `preset` nodes are always appended since several may coexist.
fn merge_key_entries(target: &mut KdlDocument, source: &KdlDocument) {
    for snode in source.nodes() {
        let name = snode.name().value();
        let nodes = target.nodes_mut();
        if name == "preset" {
            nodes.push(snode.clone());
            continue;
        }
        match nodes.iter_mut().find(|n| n.name().value() == name) {
            Some(existing) => *existing = snode.clone(),
            None => nodes.push(snode.clone()),
        }
    }
}

fn merge_page(default: &mut KdlDocument, unode: &KdlNode) -> Result<()> {
    let page_name = arg0(unode).ok_or_else(|| anyhow!("page block missing name argument"))?;
    let dpage = find_mut(default, |n| {
        n.name().value() == "page" && arg0(n) == Some(page_name)
    })
    .ok_or_else(|| anyhow!("unknown page {page_name:?}"))?;
    let Some(uchildren) = unode.children() else {
        return Ok(());
    };
    let dchildren = dpage.ensure_children();

    for name in ["layout", "tabs", "bind"] {
        replace_all_named(dchildren, uchildren, name);
    }

    for un in uchildren.nodes() {
        match un.name().value() {
            "layout" | "tabs" | "bind" => {}
            "pane" => merge_pane(dchildren, un, page_name)?,
            other => {
                return Err(anyhow!(
                    "page {page_name:?}: unknown block {other:?} \
                     (expected `layout`, `tabs`, `bind`, or `pane`)"
                ))
            }
        }
    }
    Ok(())
}

/// If `source` contains any nodes named `name`, remove every such node from
/// `target` and insert the source nodes where the first removed node was
/// (or at the end when `target` had none).
fn replace_all_named(target: &mut KdlDocument, source: &KdlDocument, name: &str) {
    let replacement: Vec<KdlNode> = source
        .nodes()
        .iter()
        .filter(|n| n.name().value() == name)
        .cloned()
        .collect();
    if replacement.is_empty() {
        return;
    }
    let nodes = target.nodes_mut();
    let pos = nodes
        .iter()
        .position(|n| n.name().value() == name)
        .unwrap_or(nodes.len());
    nodes.retain(|n| n.name().value() != name);
    let pos = pos.min(nodes.len());
    for (i, n) in replacement.into_iter().enumerate() {
        nodes.insert(pos + i, n);
    }
}

fn merge_pane(dchildren: &mut KdlDocument, upane: &KdlNode, page: &str) -> Result<()> {
    let pane_name =
        arg0(upane).ok_or_else(|| anyhow!("page {page:?}: pane block missing name argument"))?;
    let dpane = find_mut(dchildren, |n| {
        n.name().value() == "pane" && arg0(n) == Some(pane_name)
    })
    .ok_or_else(|| anyhow!("page {page:?}: unknown pane {pane_name:?}"))?;
    let Some(ublocks) = upane.children() else {
        return Ok(());
    };
    for ublock in ublocks.nodes() {
        match ublock.name().value() {
            "keys" => {
                let Some(ukeys) = ublock.children() else {
                    continue;
                };
                let dblocks = dpane.ensure_children();
                match find_mut(dblocks, |n| n.name().value() == "keys") {
                    Some(dkeys) => merge_key_entries(dkeys.ensure_children(), ukeys),
                    None => dblocks.nodes_mut().push(ublock.clone()),
                }
            }
            other => {
                return Err(anyhow!(
                    "page {page:?} pane {pane_name:?}: unknown block {other:?} (expected `keys`)"
                ))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = r#"
theme "dark"
projects-board "one"
pages "git" "github"
app {
    "Ctrl+c" "Quit"
    "1" "page:git"
}
page "git" {
    layout { place "a" }
    tabs "a"
    bind select="a" detail="b"
    pane "a" { keys { preset "nav"; "x" "One"; "y" "Two" } }
    pane "b" { keys { "z" "Three" } }
}
"#;

    fn merged(user: &str) -> Result<KdlDocument> {
        let mut d: KdlDocument = DEFAULT.parse().unwrap();
        let u: KdlDocument = user.parse().unwrap();
        merge_user_config(&mut d, &u)?;
        Ok(d)
    }

    fn page<'a>(doc: &'a KdlDocument, name: &str) -> &'a KdlDocument {
        doc.nodes()
            .iter()
            .find(|n| n.name().value() == "page" && arg0(n) == Some(name))
            .and_then(|n| n.children())
            .unwrap()
    }

    fn keys(page: &KdlDocument, pane: &str) -> Vec<(String, String)> {
        page.nodes()
            .iter()
            .find(|n| n.name().value() == "pane" && arg0(n) == Some(pane))
            .and_then(|n| n.children())
            .and_then(|c| c.nodes().iter().find(|n| n.name().value() == "keys"))
            .and_then(|k| k.children())
            .unwrap()
            .nodes()
            .iter()
            .map(|n| {
                (
                    n.name().value().to_string(),
                    arg0(n).unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn app_keys_merge_per_key() {
        let d = merged(r#"app { "1" "page:github"; "q" "Quit" }"#).unwrap();
        let app = d
            .nodes()
            .iter()
            .find(|n| n.name().value() == "app")
            .unwrap();
        let entries: Vec<(String, String)> = app
            .children()
            .unwrap()
            .nodes()
            .iter()
            .map(|n| (n.name().value().to_string(), arg0(n).unwrap().to_string()))
            .collect();
        assert_eq!(
            entries,
            vec![
                ("Ctrl+c".into(), "Quit".into()),
                ("1".into(), "page:github".into()),
                ("q".into(), "Quit".into()),
            ]
        );
    }

    #[test]
    fn pane_keys_override_add_and_keep_presets() {
        let d = merged(
            r#"page "git" { pane "a" { keys { "x" "None"; "w" "Four"; preset "search" } } }"#,
        )
        .unwrap();
        assert_eq!(
            keys(page(&d, "git"), "a"),
            vec![
                ("preset".into(), "nav".into()),
                ("x".into(), "None".into()),
                ("y".into(), "Two".into()),
                ("w".into(), "Four".into()),
                ("preset".into(), "search".into()),
            ]
        );
        // Untouched pane stays as-is.
        assert_eq!(
            keys(page(&d, "git"), "b"),
            vec![("z".into(), "Three".into())]
        );
    }

    #[test]
    fn layout_tabs_bind_replaced_wholesale() {
        let d = merged(
            r#"page "git" {
                layout { split direction="vertical" { place "b"; place "a" } }
                tabs "b" "a"
                bind select="b" detail="a"
                bind select="a" detail="b"
            }"#,
        )
        .unwrap();
        let p = page(&d, "git");
        let layout = p
            .nodes()
            .iter()
            .filter(|n| n.name().value() == "layout")
            .count();
        assert_eq!(layout, 1);
        let root = p
            .nodes()
            .iter()
            .find(|n| n.name().value() == "layout")
            .unwrap()
            .children()
            .unwrap()
            .nodes()
            .first()
            .unwrap();
        assert_eq!(root.name().value(), "split");
        let tabs: Vec<&str> = p
            .nodes()
            .iter()
            .find(|n| n.name().value() == "tabs")
            .unwrap()
            .entries()
            .iter()
            .filter_map(|e| e.value().as_string())
            .collect();
        assert_eq!(tabs, vec!["b", "a"]);
        assert_eq!(
            p.nodes()
                .iter()
                .filter(|n| n.name().value() == "bind")
                .count(),
            2
        );
        // Pane blocks (and therefore ID order) are untouched.
        let panes: Vec<&str> = p
            .nodes()
            .iter()
            .filter(|n| n.name().value() == "pane")
            .filter_map(arg0)
            .collect();
        assert_eq!(panes, vec!["a", "b"]);
    }

    #[test]
    fn theme_replaced() {
        let d = merged(r#"theme "light""#).unwrap();
        let themes: Vec<&str> = d
            .nodes()
            .iter()
            .filter(|n| n.name().value() == "theme")
            .filter_map(arg0)
            .collect();
        assert_eq!(themes, vec!["light"]);
    }

    #[test]
    fn github_poll_interval_replaced() {
        let d = merged(r#"github-poll-interval "10s""#).unwrap();
        let vals: Vec<&str> = d
            .nodes()
            .iter()
            .filter(|n| n.name().value() == "github-poll-interval")
            .filter_map(arg0)
            .collect();
        assert_eq!(vals, vec!["10s"]);
    }

    #[test]
    fn projects_board_replaced_wholesale() {
        let d = merged("projects-board 2").unwrap();
        let pins: Vec<String> = d
            .nodes()
            .iter()
            .filter(|n| n.name().value() == "projects-board")
            .map(|n| n.entries()[0].value().to_string())
            .collect();
        assert_eq!(pins, vec!["2"]);
    }

    #[test]
    fn pages_replaced_wholesale() {
        let d = merged(r#"pages "github""#).unwrap();
        let pages: Vec<Vec<&str>> = d
            .nodes()
            .iter()
            .filter(|n| n.name().value() == "pages")
            .map(|n| {
                n.entries()
                    .iter()
                    .filter_map(|e| e.value().as_string())
                    .collect()
            })
            .collect();
        assert_eq!(pages, vec![vec!["github"]]);
    }

    #[test]
    fn unknown_page_pane_and_blocks_fail() {
        let err = merged(r#"page "nope" { }"#).unwrap_err().to_string();
        assert!(err.contains("unknown page \"nope\""), "{err}");
        let err = merged(r#"page "git" { pane "nope" { keys { } } }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown pane \"nope\""), "{err}");
        let err = merged(r#"colors "x""#).unwrap_err().to_string();
        assert!(err.contains("unknown top-level block \"colors\""), "{err}");
        assert!(err.contains("`projects-board`"), "{err}");
        let err = merged(r#"page "git" { pane "a" { colors { } } }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown block \"colors\""), "{err}");
        let err = merged(r#"page "git" { layouts { } }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown block \"layouts\""), "{err}");
    }
}
