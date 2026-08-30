use crate::core::config::constraint::parse_constraint;
use crate::core::config::keymap_builder::{build_keymap, KeymapEntry};
use crate::core::config::merge::merge_user_config;
use crate::core::keymap::Keymap;
use crate::core::layout::{LayoutNode, PageLayoutConfig, SlotCase, SlotRule, SplitDirection};
use anyhow::{anyhow, Context, Result};
use kdl::{KdlDocument, KdlNode};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

static DEFAULT_KDL: &str = include_str!("../../../assets/default.kdl");

/// Used when the config has no `procs-refresh-interval` node.
const DEFAULT_PROCS_REFRESH_INTERVAL: &str = "2s";
/// Ticks fire every 250 ms, so a shorter interval cannot be honoured.
const MIN_PROCS_REFRESH_MS: u64 = 250;

/// Parse `"2s"`, `"1.5s"` or `"500ms"` into a duration of at least
/// [`MIN_PROCS_REFRESH_MS`]. `None` for anything else.
pub fn parse_interval(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (num, per_unit_ms) = s
        .strip_suffix("ms")
        .map(|n| (n, 1.0))
        .or_else(|| s.strip_suffix('s').map(|n| (n, 1000.0)))?;
    let value: f64 = num.trim().parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let ms = (value * per_unit_ms).round() as u64;
    (ms >= MIN_PROCS_REFRESH_MS).then(|| Duration::from_millis(ms))
}

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
    /// Path of the user config file this page came from, if any (for error messages).
    pub source: Option<PathBuf>,
}

impl LoadedPageConfig {
    fn describe_source(&self) -> String {
        describe_source(self.source.as_deref())
    }

    /// Build the typed keymap for `pane` from this page's `keys { }` block.
    pub fn keymap<A>(&self, pane: &str) -> Result<Keymap<A>>
    where
        A: Clone + FromStr<Err = String>,
    {
        let entries = self.pane_keys.get(pane).ok_or_else(|| {
            anyhow!(
                "invalid {}: page {:?} missing pane {pane:?} keys block",
                self.describe_source(),
                self.name
            )
        })?;
        build_keymap::<A>(entries).map_err(|e| {
            anyhow!(
                "invalid {}: page {:?} pane {pane:?}: {e}",
                self.describe_source(),
                self.name
            )
        })
    }

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

fn describe_source(source: Option<&Path>) -> String {
    match source {
        Some(p) => format!("config file {}", p.display()),
        None => "built-in config".to_string(),
    }
}

/// Pages that were removed by folding them into another page. Configs that
/// still name them get told where the page went instead of "unknown page".
fn removed_page_reason(name: &str) -> Option<String> {
    match name {
        "actions" => Some(
            "page \"actions\" was folded into the \"github\" page (v0.8.0); \
             remove it from pages / app bindings"
                .to_string(),
        ),
        _ => None,
    }
}

// ── Config ────────────────────────────────────────────────────────────────────

/// The effective configuration: the built-in defaults with an optional user
/// document merged on top (see `merge.rs` for the rules).
pub struct Config {
    /// Pristine built-in defaults. Pane IDs are always derived from this
    /// document so a user layout can never renumber panes.
    default_doc: KdlDocument,
    /// Defaults with the user document merged in.
    doc: KdlDocument,
    /// Path of the user config file, if any (for error messages).
    source: Option<PathBuf>,
}

impl Config {
    /// The raw text of the embedded default config.
    pub fn default_text() -> &'static str {
        DEFAULT_KDL
    }

    fn parse_default() -> KdlDocument {
        DEFAULT_KDL.parse().expect("default.kdl is always valid")
    }

    /// Built-in defaults only.
    pub fn builtin() -> Self {
        let default_doc = Self::parse_default();
        Self {
            doc: default_doc.clone(),
            default_doc,
            source: None,
        }
    }

    /// Defaults with `user` merged on top. Structural errors (unknown pages,
    /// panes, incomplete layouts, …) are reported immediately.
    pub fn with_user(user: &KdlDocument, source: PathBuf) -> Result<Self> {
        let default_doc = Self::parse_default();
        let mut doc = default_doc.clone();
        merge_user_config(&mut doc, user)
            .with_context(|| format!("invalid config file {}", source.display()))?;
        let cfg = Self {
            default_doc,
            doc,
            source: Some(source),
        };
        let pages = cfg.pages()?;
        cfg.validate_user_app_bindings(user, &pages)?;
        cfg.git_page()?;
        cfg.github_page()?;
        cfg.files_page()?;
        cfg.docker_page()?;
        cfg.procs_page()?;
        cfg.worktrees_page()?;
        cfg.projects_page()?;
        cfg.app_entries()?;
        cfg.theme()?;
        cfg.icons()?;
        cfg.image_preview()?;
        cfg.procs_refresh_interval()?;
        Ok(cfg)
    }

    /// Every page the built-in config defines, in `page "<name>"` declaration
    /// order. This is the set of names `pages` may list.
    pub fn builtin_page_names(&self) -> Vec<String> {
        self.default_doc
            .nodes()
            .iter()
            .filter(|n| n.name().value() == "page")
            .filter_map(|n| n.get(0usize).and_then(|v| v.as_string()))
            .map(str::to_string)
            .collect()
    }

    /// The enabled pages in slot order (`pages "git" "files" ...`): the
    /// position in this list is the tab position shown in the header.
    /// Pages not listed are disabled. Falls back to every built-in page in
    /// declaration order when the node is absent.
    pub fn pages(&self) -> Result<Vec<String>> {
        let known = self.builtin_page_names();
        let Some(node) = self
            .doc
            .nodes()
            .iter()
            .find(|n| n.name().value() == "pages")
        else {
            return Ok(known);
        };
        let mut pages: Vec<String> = Vec::new();
        for entry in node.entries() {
            let name = match (entry.name(), entry.value().as_string()) {
                (None, Some(s)) => s,
                _ => {
                    return Err(anyhow!(
                        "invalid {}: pages: expected page names as string arguments \
                         (e.g. `pages \"git\" \"files\"`)",
                        self.describe()
                    ))
                }
            };
            if !known.iter().any(|k| k == name) {
                if let Some(reason) = removed_page_reason(name) {
                    return Err(anyhow!("invalid {}: pages: {reason}", self.describe()));
                }
                return Err(anyhow!(
                    "invalid {}: pages: unknown page {name:?}; expected one of: {}",
                    self.describe(),
                    known.join(", ")
                ));
            }
            if pages.iter().any(|p| p == name) {
                return Err(anyhow!(
                    "invalid {}: pages: page {name:?} listed twice",
                    self.describe()
                ));
            }
            pages.push(name.to_string());
        }
        if pages.is_empty() {
            return Err(anyhow!(
                "invalid {}: pages must list at least one page; expected some of: {}",
                self.describe(),
                known.join(", ")
            ));
        }
        Ok(pages)
    }

    /// A `page:<name>` binding written by the user must address an enabled
    /// page. (Built-in bindings to pages the user disabled are dropped by
    /// [`app_entries`](Self::app_entries) instead, so disabling a page does
    /// not require unbinding its default key.)
    fn validate_user_app_bindings(&self, user: &KdlDocument, pages: &[String]) -> Result<()> {
        let Some(app) = user
            .nodes()
            .iter()
            .find(|n| n.name().value() == "app")
            .and_then(|n| n.children())
        else {
            return Ok(());
        };
        let known = self.builtin_page_names();
        for (key, action) in parse_app_block(app)
            .with_context(|| format!("invalid {}: app block", self.describe()))?
        {
            let Some(name) = action.strip_prefix("page:") else {
                continue;
            };
            if pages.iter().any(|p| p == name) {
                continue;
            }
            let reason = if known.iter().any(|k| k == name) {
                format!(
                    "page {name:?} is not listed in `pages` ({})",
                    pages.join(", ")
                )
            } else if let Some(reason) = removed_page_reason(name) {
                reason
            } else {
                format!(
                    "unknown page {name:?}; expected one of: {}",
                    known.join(", ")
                )
            };
            return Err(anyhow!(
                "invalid {}: app block: {key:?} {action:?}: {reason}",
                self.describe()
            ));
        }
        Ok(())
    }

    /// How often the Procs view re-reads processes and ports
    /// (`procs-refresh-interval "2s"`; also `"500ms"`, `"1.5s"`; at least 250 ms).
    pub fn procs_refresh_interval(&self) -> Result<Duration> {
        let raw = self
            .doc
            .nodes()
            .iter()
            .find(|n| n.name().value() == "procs-refresh-interval")
            .map(|n| {
                n.get(0usize)
                    .and_then(|v| v.as_string())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        anyhow!("procs-refresh-interval block missing interval argument")
                    })
            })
            .transpose()
            .with_context(|| format!("invalid {}", self.describe()))?
            .unwrap_or_else(|| DEFAULT_PROCS_REFRESH_INTERVAL.to_string());
        parse_interval(&raw).ok_or_else(|| {
            anyhow!(
                "invalid {}: bad procs-refresh-interval {raw:?}; expected a duration such as \
                 \"2s\" or \"500ms\" (at least {}ms)",
                self.describe(),
                MIN_PROCS_REFRESH_MS
            )
        })
    }

    /// How the Files view previews images (`image-preview "auto"` / `"halfblocks"` / `"none"`).
    pub fn image_preview(&self) -> Result<crate::files::domain::image::ImagePreviewMode> {
        use crate::files::domain::image::{ImagePreviewMode, IMAGE_PREVIEW_MODES};
        let mode = self
            .doc
            .nodes()
            .iter()
            .find(|n| n.name().value() == "image-preview")
            .map(|n| {
                n.get(0usize)
                    .and_then(|v| v.as_string())
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("image-preview block missing mode argument"))
            })
            .transpose()
            .with_context(|| format!("invalid {}", self.describe()))?
            .unwrap_or_else(|| IMAGE_PREVIEW_MODES[0].to_string());
        ImagePreviewMode::parse(&mode).ok_or_else(|| {
            anyhow!(
                "invalid {}: unknown image-preview mode {mode:?}; expected one of: {}",
                self.describe(),
                IMAGE_PREVIEW_MODES.join(", ")
            )
        })
    }

    /// Whether the Files view shows Nerd Font icons (`icons "nerd"` / `"none"`).
    pub fn icons(&self) -> Result<bool> {
        let modes = crate::files::domain::icons::ICON_MODES;
        let mode = self
            .doc
            .nodes()
            .iter()
            .find(|n| n.name().value() == "icons")
            .map(|n| {
                n.get(0usize)
                    .and_then(|v| v.as_string())
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("icons block missing mode argument"))
            })
            .transpose()
            .with_context(|| format!("invalid {}", self.describe()))?
            .unwrap_or_else(|| modes[0].to_string());
        if !modes.contains(&mode.as_str()) {
            return Err(anyhow!(
                "invalid {}: unknown icons mode {mode:?}; expected one of: {}",
                self.describe(),
                modes.join(", ")
            ));
        }
        Ok(mode == "nerd")
    }

    /// Syntax highlighting theme name, validated against the bundled themes.
    pub fn theme(&self) -> Result<String> {
        let name = self
            .doc
            .nodes()
            .iter()
            .find(|n| n.name().value() == "theme")
            .map(|n| {
                n.get(0usize)
                    .and_then(|v| v.as_string())
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("theme block missing name argument"))
            })
            .transpose()
            .with_context(|| format!("invalid {}", self.describe()))?
            .unwrap_or_else(|| crate::core::syntax::DEFAULT_THEME.to_string());
        let available = crate::core::syntax::theme_names();
        if !available.contains(&name) {
            return Err(anyhow!(
                "invalid {}: unknown theme {name:?}; available: {}",
                self.describe(),
                available.join(", ")
            ));
        }
        Ok(name)
    }

    /// Human-readable origin for error messages.
    pub fn describe(&self) -> String {
        describe_source(self.source.as_deref())
    }

    pub fn git_page(&self) -> Result<LoadedPageConfig> {
        self.page("git")
    }

    pub fn github_page(&self) -> Result<LoadedPageConfig> {
        self.page("github")
    }

    pub fn files_page(&self) -> Result<LoadedPageConfig> {
        self.page("files")
    }

    pub fn docker_page(&self) -> Result<LoadedPageConfig> {
        self.page("docker")
    }

    pub fn procs_page(&self) -> Result<LoadedPageConfig> {
        self.page("procs")
    }

    pub fn worktrees_page(&self) -> Result<LoadedPageConfig> {
        self.page("worktrees")
    }

    pub fn projects_page(&self) -> Result<LoadedPageConfig> {
        self.page("projects")
    }

    fn page(&self, name: &str) -> Result<LoadedPageConfig> {
        load_page_from_doc(&self.doc, &self.default_doc, name)
            .map(|mut p| {
                p.source = self.source.clone();
                p
            })
            .with_context(|| format!("invalid {}", self.describe()))
    }

    /// App-level `(key_str, action_str)` pairs. `page:<name>` entries whose
    /// page exists but is not enabled by [`pages`](Self::pages) are dropped,
    /// so the built-in slot keys of disabled pages simply do nothing.
    pub fn app_entries(&self) -> Result<Vec<(String, String)>> {
        let entries = self
            .doc
            .nodes()
            .iter()
            .find(|n| n.name().value() == "app")
            .and_then(|n| n.children())
            .map(parse_app_block)
            .unwrap_or_else(|| Ok(Vec::new()))
            .with_context(|| format!("invalid {}: app block", self.describe()))?;
        let pages = self.pages()?;
        let known = self.builtin_page_names();
        Ok(entries
            .into_iter()
            .filter(|(_, action)| match action.strip_prefix("page:") {
                Some(name) => pages.iter().any(|p| p == name) || !known.iter().any(|k| k == name),
                None => true,
            })
            .collect())
    }
}

// ── Built-in convenience loaders (tests) ──────────────────────────────────────

/// Parse the git page from the embedded default config.
#[cfg(test)]
pub fn load_git_page_config() -> Result<LoadedPageConfig> {
    Config::builtin().git_page()
}

/// Parse the github page from the embedded default config.
#[cfg(test)]
pub fn load_github_page_config() -> Result<LoadedPageConfig> {
    Config::builtin().github_page()
}

/// Parse app-level key entries from the embedded default config.
#[cfg(test)]
pub fn load_app_entries() -> Vec<(String, String)> {
    Config::builtin()
        .app_entries()
        .expect("default.kdl app block is always valid")
}

fn page_children<'a>(doc: &'a KdlDocument, page_name: &str) -> Result<&'a KdlDocument> {
    doc.nodes()
        .iter()
        .filter(|n| n.name().value() == "page")
        .find(|n| n.get(0usize).and_then(|v| v.as_string()) == Some(page_name))
        .ok_or_else(|| anyhow!("page {page_name:?} not found"))?
        .children()
        .ok_or_else(|| anyhow!("page {page_name:?} has no children block"))
}

/// The `layout { }` block's children, and its single root element.
fn layout_block<'a>(
    children: &'a KdlDocument,
    page_name: &str,
) -> Result<(&'a KdlDocument, &'a KdlNode)> {
    let layout_doc = children
        .nodes()
        .iter()
        .find(|n| n.name().value() == "layout")
        .and_then(|n| n.children())
        .ok_or_else(|| anyhow!("page {page_name:?} missing layout block"))?;
    let root = layout_doc
        .nodes()
        .first()
        .ok_or_else(|| anyhow!("page {page_name:?} layout is empty"))?;
    Ok((layout_doc, root))
}

fn load_page_from_doc(
    doc: &KdlDocument,
    default_doc: &KdlDocument,
    page_name: &str,
) -> Result<LoadedPageConfig> {
    let children = page_children(doc, page_name)?;
    let (layout_doc, layout_root) = layout_block(children, page_name)?;

    // Pane IDs come from the *default* layout's pane set, in `pane` block
    // declaration order, so a user layout can rearrange panes without
    // renumbering them.
    let default_children = page_children(default_doc, page_name)?;
    let (_, default_layout_root) = layout_block(default_children, page_name)?;
    let default_layout_names = collect_layout_pane_names(default_layout_root);
    let pane_ids = build_pane_ids(children, &default_layout_names);

    // Panes are compile-time fixed, so a layout may rearrange them but not
    // drop them: every pane of the page must be placed somewhere.
    let placed = collect_layout_pane_names(layout_root);
    let missing: Vec<&str> = default_layout_names
        .iter()
        .filter(|n| !placed.contains(n))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(anyhow!(
            "page {page_name:?}: layout must place every pane of the page; missing: {missing:?}"
        ));
    }

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
        source: None,
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
                    match child.name().value() {
                        "triggers" => {
                            for entry in child.entries().iter().filter(|e| e.name().is_none()) {
                                if let Some(name) = entry.value().as_string() {
                                    names.push(name.to_string());
                                }
                            }
                        }
                        "when" => {
                            if let Some(name) = child.get("then").and_then(|v| v.as_string()) {
                                names.push(name.to_string());
                            }
                            for entry in child.entries().iter().filter(|e| e.name().is_none()) {
                                if let Some(name) = entry.value().as_string() {
                                    names.push(name.to_string());
                                }
                            }
                        }
                        _ => {}
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
                if layout_names.iter().any(|n| n == name) {
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

    let default_name = node
        .get("default")
        .and_then(|v| v.as_string())
        .ok_or_else(|| anyhow!("slot {slot_name:?} missing default="))?;
    let default_pane = *name_map
        .get(default_name)
        .ok_or_else(|| anyhow!("unknown pane in slot default=: {default_name:?}"))?;

    let resolve_pane = |pane_name: &str, what: &str| -> Result<usize> {
        name_map
            .get(pane_name)
            .copied()
            .ok_or_else(|| anyhow!("unknown pane in {what}: {pane_name:?}"))
    };
    let trigger_list = |entries: &[kdl::KdlEntry], what: &str| -> Result<Vec<usize>> {
        entries
            .iter()
            .filter(|e| e.name().is_none())
            .map(|e| {
                let pane_name = e
                    .value()
                    .as_string()
                    .ok_or_else(|| anyhow!("{what} entry is not a string"))?;
                resolve_pane(pane_name, what)
            })
            .collect()
    };

    let children: Vec<&KdlNode> = node
        .children()
        .map(|d| d.nodes().iter().collect())
        .unwrap_or_default();

    // Single-case shorthand: `then=` on the slot plus a `triggers` child.
    let mut cases = Vec::new();
    if let Some(then_name) = node.get("then").and_then(|v| v.as_string()) {
        let then_pane = resolve_pane(then_name, "slot then=")?;
        let trigger_panes = children
            .iter()
            .find(|n| n.name().value() == "triggers")
            .map(|n| trigger_list(n.entries(), "triggers"))
            .transpose()?
            .unwrap_or_default();
        cases.push(SlotCase {
            trigger_panes,
            then_pane,
        });
    }
    // Multi-case form: `when "<pane>" ... then="<pane>"` children, in order.
    for child in children.iter().filter(|n| n.name().value() == "when") {
        let then_name = child
            .get("then")
            .and_then(|v| v.as_string())
            .ok_or_else(|| anyhow!("slot {slot_name:?}: when missing then="))?;
        let then_pane = resolve_pane(then_name, "when then=")?;
        let trigger_panes = trigger_list(child.entries(), "when")?;
        if trigger_panes.is_empty() {
            return Err(anyhow!(
                "slot {slot_name:?}: when then={then_name:?} lists no trigger panes"
            ));
        }
        cases.push(SlotCase {
            trigger_panes,
            then_pane,
        });
    }
    if cases.is_empty() {
        return Err(anyhow!(
            "slot {slot_name:?} needs then= (with a triggers child) or at least one when child"
        ));
    }

    slot_rules.push(SlotRule {
        slot_id,
        cases,
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
    let Some(pane_children) = pane_node.children() else {
        return Ok(Vec::new());
    };

    // A pane may have no bindings at all (display-only panes such as the
    // Files page's parent directory column).
    let Some(keys_children) = pane_children
        .nodes()
        .iter()
        .find(|n| n.name().value() == "keys")
        .and_then(|n| n.children())
    else {
        return Ok(Vec::new());
    };

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
        for name in &[
            "view",
            "issue_list",
            "pr_list",
            "run_list",
            "issue_detail",
            "pr_detail",
            "run_detail",
        ] {
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
    fn files_page_ids_keys_and_bindings() {
        let cfg = Config::builtin().files_page().unwrap();
        assert_eq!(cfg.name, "files");
        assert_eq!(cfg.resolve_id("parent_dir"), Some(0));
        assert_eq!(cfg.resolve_id("dir_list"), Some(1));
        assert_eq!(cfg.resolve_id("preview"), Some(2));
        assert_eq!(cfg.layout.tab_panes, vec![1, 2]);
        assert_eq!(
            cfg.bindings,
            vec![("dir_list".to_string(), "preview".to_string())]
        );
        for name in ["view", "parent_dir", "dir_list", "preview"] {
            assert!(
                cfg.pane_keys.contains_key(name),
                "missing pane keys for {name}"
            );
        }
        // Display-only pane: a `pane` block without keys is allowed and empty.
        assert!(cfg.pane_keys["parent_dir"].is_empty());
        // App block switches to it with "3".
        let entries = Config::builtin().app_entries().unwrap();
        assert!(entries.contains(&("3".to_string(), "page:files".to_string())));
    }

    #[test]
    fn docker_page_ids_keys_and_bindings() {
        let cfg = Config::builtin().docker_page().unwrap();
        assert_eq!(cfg.name, "docker");
        assert_eq!(cfg.resolve_id("containers"), Some(0));
        assert_eq!(cfg.resolve_id("images"), Some(1));
        assert_eq!(cfg.resolve_id("detail"), Some(2));
        assert_eq!(cfg.resolve_id("logs"), Some(3));
        assert_eq!(cfg.layout.tab_panes, vec![0, 1, 2, 3]);
        assert_eq!(
            cfg.bindings,
            vec![
                ("containers".to_string(), "detail".to_string()),
                ("images".to_string(), "detail".to_string()),
            ]
        );
        for name in ["view", "containers", "images", "detail", "logs"] {
            assert!(
                cfg.pane_keys.contains_key(name),
                "missing pane keys for {name}"
            );
        }
        // App block switches to it with "4".
        let entries = Config::builtin().app_entries().unwrap();
        assert!(entries.contains(&("4".to_string(), "page:docker".to_string())));
    }

    #[test]
    fn projects_page_ids_keys_and_bindings() {
        let cfg = Config::builtin().projects_page().unwrap();
        assert_eq!(cfg.name, "projects");
        assert_eq!(cfg.resolve_id("projects"), Some(0));
        assert_eq!(cfg.resolve_id("board"), Some(1));
        assert_eq!(cfg.resolve_id("detail"), Some(2));
        assert_eq!(cfg.layout.tab_panes, vec![0, 1, 2]);
        assert_eq!(
            cfg.bindings,
            vec![
                ("projects".to_string(), "board".to_string()),
                ("board".to_string(), "detail".to_string()),
            ]
        );
        for name in ["view", "projects", "board", "detail"] {
            assert!(
                cfg.pane_keys.contains_key(name),
                "missing pane keys for {name}"
            );
        }
        let board = format!("{:?}", cfg.pane_keys["board"]);
        for (k, a) in [
            ("t", "ToggleTable"),
            ("s", "CycleSort"),
            ("h", "PrevColumn"),
        ] {
            assert!(
                board.contains(&format!("key: {k:?}, action: {a:?}")),
                "board pane binds {k} to {a}: {board}"
            );
        }
        // App block switches to it with "7".
        let entries = Config::builtin().app_entries().unwrap();
        assert!(entries.contains(&("7".to_string(), "page:projects".to_string())));
    }

    #[test]
    fn procs_page_ids_keys_and_bindings() {
        let cfg = Config::builtin().procs_page().unwrap();
        assert_eq!(cfg.name, "procs");
        assert_eq!(cfg.resolve_id("processes"), Some(0));
        assert_eq!(cfg.resolve_id("ports"), Some(1));
        assert_eq!(cfg.resolve_id("detail"), Some(2));
        assert_eq!(cfg.layout.tab_panes, vec![0, 1, 2]);
        assert_eq!(
            cfg.bindings,
            vec![("processes".to_string(), "detail".to_string())]
        );
        for name in ["view", "processes", "ports", "detail"] {
            assert!(
                cfg.pane_keys.contains_key(name),
                "missing pane keys for {name}"
            );
        }
        // App block switches to it with "5".
        let entries = Config::builtin().app_entries().unwrap();
        assert!(entries.contains(&("5".to_string(), "page:procs".to_string())));
    }

    #[test]
    fn procs_refresh_interval_default_override_and_validation() {
        assert_eq!(
            Config::builtin().procs_refresh_interval().unwrap(),
            Duration::from_secs(2)
        );
        assert_eq!(
            user(r#"procs-refresh-interval "500ms""#)
                .unwrap()
                .procs_refresh_interval()
                .unwrap(),
            Duration::from_millis(500)
        );
        assert_eq!(
            user(r#"procs-refresh-interval "1.5s""#)
                .unwrap()
                .procs_refresh_interval()
                .unwrap(),
            Duration::from_millis(1500)
        );
        for bad in [
            r#"procs-refresh-interval "fast""#,
            r#"procs-refresh-interval "100ms""#,
        ] {
            let msg = format!("{:#}", user(bad).err().expect("expected an error"));
            assert!(msg.contains("/u/config.kdl"), "{bad}: {msg}");
            assert!(msg.contains("procs-refresh-interval"), "{bad}: {msg}");
        }
        assert_eq!(parse_interval("2s"), Some(Duration::from_secs(2)));
        assert_eq!(parse_interval(" 250ms "), Some(Duration::from_millis(250)));
        assert_eq!(parse_interval("0s"), None);
        assert_eq!(parse_interval("-1s"), None);
        assert_eq!(parse_interval("2"), None);
    }

    #[test]
    fn github_page_runs_column_ids_keys_and_bindings() {
        let cfg = Config::builtin().github_page().unwrap();
        assert_eq!(cfg.resolve_id("run_list"), Some(2));
        assert_eq!(cfg.resolve_id("run_detail"), Some(5));
        // The three list columns are the tab panes; detail panes are reached
        // with Enter and left with Esc.
        assert_eq!(cfg.layout.tab_panes, vec![0, 1, 2]);
        assert_eq!(
            cfg.bindings,
            vec![
                ("issue_list".to_string(), "issue_detail".to_string()),
                ("pr_list".to_string(), "pr_detail".to_string()),
                ("run_list".to_string(), "run_detail".to_string()),
            ]
        );
        // One detail slot with a case per detail pane.
        assert_eq!(cfg.layout.slot_rules.len(), 1);
        let rule = &cfg.layout.slot_rules[0];
        assert_eq!(rule.default_pane, 3, "issue_detail by default");
        assert_eq!(rule.resolve(1), 4, "pr_list focus shows pr_detail");
        assert_eq!(rule.resolve(4), 4);
        assert_eq!(rule.resolve(2), 5, "run_list focus shows run_detail");
        assert_eq!(rule.resolve(5), 5);
        assert_eq!(rule.resolve(0), 3);
        // The Actions page is gone: no page block, no key.
        assert!(Config::builtin()
            .builtin_page_names()
            .iter()
            .all(|p| p != "actions"));
        let entries = Config::builtin().app_entries().unwrap();
        assert!(entries.iter().all(|(_, a)| a != "page:actions"));
    }

    #[test]
    fn removed_actions_page_fails_fast_with_a_hint() {
        for bad in [
            r#"pages "git" "actions""#,
            r#"app { "6" "page:actions" }"#,
            r#"pages "git" "github"; app { "a" "page:actions" }"#,
        ] {
            let msg = format!("{:#}", user(bad).err().expect("expected an error"));
            assert!(msg.contains("/u/config.kdl"), "{bad}: {msg}");
            assert!(
                msg.contains("folded into the \"github\" page"),
                "{bad}: {msg}"
            );
            assert!(msg.contains("v0.8.0"), "{bad}: {msg}");
            assert!(
                msg.contains("remove it from pages / app bindings"),
                "{bad}: {msg}"
            );
        }
        // A user page block for it is a plain unknown page.
        let msg = format!(
            "{:#}",
            user(r#"page "actions" { }"#)
                .err()
                .expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
    }

    #[test]
    fn worktrees_page_ids_keys_and_bindings() {
        let cfg = Config::builtin().worktrees_page().unwrap();
        assert_eq!(cfg.name, "worktrees");
        assert_eq!(cfg.resolve_id("worktrees"), Some(0));
        assert_eq!(cfg.resolve_id("stashes"), Some(1));
        assert_eq!(cfg.resolve_id("preview"), Some(2));
        assert_eq!(cfg.layout.tab_panes, vec![0, 1, 2]);
        assert_eq!(
            cfg.bindings,
            vec![
                ("worktrees".to_string(), "preview".to_string()),
                ("stashes".to_string(), "preview".to_string()),
            ]
        );
        // Both lists drive the same preview pane.
        let select = cfg.resolve_select_bindings();
        assert_eq!(select.get(&0), Some(&2));
        assert_eq!(select.get(&1), Some(&2));
        for name in ["view", "worktrees", "stashes", "preview"] {
            assert!(
                cfg.pane_keys.contains_key(name),
                "missing pane keys for {name}"
            );
        }
        // App block switches to it with "6".
        let entries = Config::builtin().app_entries().unwrap();
        assert!(entries.contains(&("6".to_string(), "page:worktrees".to_string())));
    }

    #[test]
    fn github_pane_ids_correct() {
        let cfg = load_github_page_config().unwrap();
        let expected = [
            ("issue_list", 0usize),
            ("pr_list", 1),
            ("run_list", 2),
            ("issue_detail", 3),
            ("pr_detail", 4),
            ("run_detail", 5),
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

    // ── User config overlay ───────────────────────────────────────────────

    fn user(kdl: &str) -> Result<Config> {
        let doc: KdlDocument = kdl.parse().unwrap();
        Config::with_user(&doc, PathBuf::from("/u/config.kdl"))
    }

    fn key(s: &str) -> crossterm::event::KeyEvent {
        let ki: crate::core::keymap::KeyInput = s.parse().unwrap();
        crossterm::event::KeyEvent::new(ki.code, ki.modifiers)
    }

    #[test]
    fn user_overrides_and_unbinds_pane_keys() {
        use crate::git::panes::file_tree::FileTreeAction;
        let cfg = user(
            r#"page "git" { pane "file_tree" { keys { "Space" "ExpandOrOpen"; "i" "None"; "x" "ToggleDir" } } }"#,
        )
        .unwrap();
        let km = cfg
            .git_page()
            .unwrap()
            .keymap::<FileTreeAction>("file_tree")
            .unwrap();
        assert!(matches!(
            km.lookup(key("Space")),
            Some(FileTreeAction::ExpandOrOpen)
        ));
        assert!(km.lookup(key("i")).is_none(), "i should be unbound");
        assert!(matches!(
            km.lookup(key("x")),
            Some(FileTreeAction::ToggleDir)
        ));
        // Preset keys and untouched bindings survive.
        assert!(km.lookup(key("j")).is_some());
        assert!(matches!(
            km.lookup(key("Enter")),
            Some(FileTreeAction::ExpandOrOpen)
        ));
        // Other panes are untouched.
        let default = Config::builtin().git_page().unwrap();
        let merged = cfg.git_page().unwrap();
        assert_eq!(
            format!("{:?}", default.pane_keys["branch_list"]),
            format!("{:?}", merged.pane_keys["branch_list"])
        );
    }

    #[test]
    fn user_app_keys_merge() {
        let cfg = user(r#"app { "3" "page:github"; "1" "None" }"#).unwrap();
        let entries = cfg.app_entries().unwrap();
        let pages = cfg.pages().unwrap();
        let names: Vec<&str> = pages.iter().map(String::as_str).collect();
        let km = crate::core::keymap::build_app_keymap(&entries, &names).unwrap();
        assert!(km.lookup(key("1")).is_none());
        assert!(matches!(
            km.lookup(key("2")),
            Some(crate::core::keymap::AppAction::SwitchPage(1))
        ));
        assert!(matches!(
            km.lookup(key("3")),
            Some(crate::core::keymap::AppAction::SwitchPage(1))
        ));
        assert!(matches!(
            km.lookup(key("Ctrl+c")),
            Some(crate::core::keymap::AppAction::Quit)
        ));
    }

    // ── pages ─────────────────────────────────────────────────────────────

    const ALL_PAGES: [&str; 7] = [
        "git",
        "github",
        "files",
        "docker",
        "procs",
        "worktrees",
        "projects",
    ];

    #[test]
    fn builtin_pages_list_every_page_in_declaration_order() {
        let cfg = Config::builtin();
        assert_eq!(cfg.pages().unwrap(), ALL_PAGES);
        assert_eq!(
            cfg.builtin_page_names(),
            ALL_PAGES,
            "the `pages` node must list every `page` block, in order"
        );
        // A config without the node falls back to the same list.
        assert_eq!(
            user("theme \"base16-eighties.dark\"")
                .unwrap()
                .pages()
                .unwrap(),
            ALL_PAGES
        );
    }

    #[test]
    fn user_pages_reorder_and_disable() {
        let cfg = user(r#"pages "files" "git""#).unwrap();
        assert_eq!(cfg.pages().unwrap(), ["files", "git"]);
        // Built-in bindings to disabled pages are dropped; the rest keep
        // addressing their page by name.
        let entries = cfg.app_entries().unwrap();
        let page_actions: Vec<&str> = entries
            .iter()
            .filter_map(|(_, a)| a.strip_prefix("page:"))
            .collect();
        assert_eq!(page_actions, ["git", "files"]);
        let pages = cfg.pages().unwrap();
        let names: Vec<&str> = pages.iter().map(String::as_str).collect();
        let km = crate::core::keymap::build_app_keymap(&entries, &names).unwrap();
        assert!(matches!(
            km.lookup(key("1")),
            Some(crate::core::keymap::AppAction::SwitchPage(1))
        ));
        assert!(matches!(
            km.lookup(key("3")),
            Some(crate::core::keymap::AppAction::SwitchPage(0))
        ));
        assert!(km.lookup(key("2")).is_none(), "github is disabled");
    }

    #[test]
    fn user_pages_errors_mention_file_and_page() {
        let msg = format!(
            "{:#}",
            user(r#"pages "git" "nope""#)
                .err()
                .expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("unknown page \"nope\""), "{msg}");
        assert!(
            msg.contains("worktrees"),
            "{msg} should list the known pages"
        );

        let msg = format!(
            "{:#}",
            user(r#"pages "git" "files" "git""#)
                .err()
                .expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("\"git\" listed twice"), "{msg}");

        let msg = format!("{:#}", user(r#"pages"#).err().expect("expected an error"));
        assert!(msg.contains("at least one page"), "{msg}");

        let msg = format!(
            "{:#}",
            user(r#"pages git=1"#).err().expect("expected an error")
        );
        assert!(msg.contains("string arguments"), "{msg}");
    }

    #[test]
    fn user_binding_to_disabled_or_unknown_page_fails() {
        let msg = format!(
            "{:#}",
            user(r#"pages "git" "files"; app { "w" "page:worktrees" }"#)
                .err()
                .expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("\"w\" \"page:worktrees\""), "{msg}");
        assert!(msg.contains("not listed in `pages`"), "{msg}");

        let msg = format!(
            "{:#}",
            user(r#"app { "x" "page:nope" }"#)
                .err()
                .expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("unknown page \"nope\""), "{msg}");

        // Rebinding an enabled page is fine even when others are disabled.
        let cfg = user(r#"pages "git" "worktrees"; app { "w" "page:worktrees" }"#).unwrap();
        assert_eq!(cfg.pages().unwrap(), ["git", "worktrees"]);
    }

    #[test]
    fn user_layout_replaces_without_renumbering_panes() {
        let cfg = user(
            r#"page "git" {
                layout {
                    split direction="horizontal" {
                        split direction="vertical" size="30%" {
                            place "file_tree"
                            place "branch_list"
                            place "reflog"
                        }
                        slot "main" then="git_log" default="diff_view" {
                            triggers "branch_list" "reflog" "git_log"
                        }
                    }
                }
                tabs "branch_list" "file_tree" "reflog"
            }"#,
        )
        .unwrap();
        let page = cfg.git_page().unwrap();
        let default = Config::builtin().git_page().unwrap();
        assert_eq!(page.pane_ids, default.pane_ids, "IDs must not change");
        assert_eq!(page.bindings, default.bindings, "bind left untouched");
        assert!(matches!(
            page.layout.tree,
            LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ..
            }
        ));
        assert_eq!(page.layout.tab_panes, vec![1, 0, 3]);
    }

    #[test]
    fn user_slot_accepts_single_and_multi_case_forms() {
        // Single-case shorthand (then= + triggers) on the github page.
        let cfg = user(
            r#"page "github" {
                layout {
                    split direction="vertical" {
                        split direction="horizontal" size="40%" {
                            place "issue_list"; place "pr_list"; place "run_list"
                        }
                        slot "detail" then="pr_detail" default="issue_detail" {
                            triggers "pr_list" "pr_detail" "run_list" "run_detail"
                        }
                    }
                }
            }"#,
        )
        .unwrap();
        let rule = &cfg.github_page().unwrap().layout.slot_rules[0];
        assert_eq!(rule.cases.len(), 1);
        assert_eq!(rule.resolve(2), 4);
        assert_eq!(rule.resolve(0), 3);

        // Multi-case form with a different order and default.
        let cfg = user(
            r#"page "github" {
                layout {
                    split direction="vertical" {
                        split direction="horizontal" size="40%" {
                            place "issue_list"; place "pr_list"; place "run_list"
                        }
                        slot "detail" default="run_detail" {
                            when "issue_list" "issue_detail" then="issue_detail"
                            when "pr_list" "pr_detail" then="pr_detail"
                        }
                    }
                }
            }"#,
        )
        .unwrap();
        let rule = &cfg.github_page().unwrap().layout.slot_rules[0];
        assert_eq!(rule.cases.len(), 2);
        assert_eq!(rule.resolve(0), 3);
        assert_eq!(rule.resolve(1), 4);
        assert_eq!(rule.resolve(2), 5, "default");

        for (bad, expect) in [
            (r#"slot "detail" default="issue_detail" { }"#, "needs then="),
            (
                r#"slot "detail" default="issue_detail" { when then="pr_detail" }"#,
                "lists no trigger panes",
            ),
            (
                r#"slot "detail" default="issue_detail" { when "pr_list" }"#,
                "when missing then=",
            ),
            (
                r#"slot "detail" default="issue_detail" { when "nope" then="pr_detail" }"#,
                "unknown pane in when",
            ),
        ] {
            let kdl = format!(
                r#"page "github" {{ layout {{ split direction="vertical" {{
                    split direction="horizontal" {{
                        place "issue_list"; place "pr_list"; place "run_list"
                        place "pr_detail"; place "run_detail"
                    }}
                    {bad}
                }} }} }}"#
            );
            let msg = format!("{:#}", user(&kdl).err().expect("expected an error"));
            assert!(msg.contains("/u/config.kdl"), "{bad}: {msg}");
            assert!(msg.contains(expect), "{bad}: {msg}");
        }
    }

    #[test]
    fn user_layout_must_place_every_pane() {
        let err = user(
            r#"page "git" {
                layout { split direction="vertical" { place "file_tree"; place "diff_view" } }
            }"#,
        )
        .err()
        .expect("expected an error");
        let msg = format!("{err:#}");
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("missing"), "{msg}");
        for pane in ["branch_list", "git_log", "reflog"] {
            assert!(msg.contains(pane), "{msg} should mention {pane}");
        }
    }

    #[test]
    fn user_structural_errors_mention_file() {
        for bad in [
            r#"page "nope" { }"#,
            r#"page "git" { pane "nope" { keys { } } }"#,
            r#"page "git" { layout { place "nope" } }"#,
            r#"page "git" { tabs "nope" }"#,
            r#"page "git" { bind select="file_tree" detail="nope" }"#,
            r#"colors { }"#,
        ] {
            let msg = format!("{:#}", user(bad).err().expect("expected an error"));
            assert!(msg.contains("/u/config.kdl"), "{bad}: {msg}");
        }
    }

    #[test]
    fn user_invalid_action_mentions_file_page_and_pane() {
        use crate::git::panes::file_tree::FileTreeAction;
        let cfg =
            user(r#"page "git" { pane "file_tree" { keys { "x" "NoSuchAction" } } }"#).unwrap();
        let msg = format!(
            "{:#}",
            cfg.git_page()
                .unwrap()
                .keymap::<FileTreeAction>("file_tree")
                .err()
                .expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("\"git\""), "{msg}");
        assert!(msg.contains("\"file_tree\""), "{msg}");
        assert!(msg.contains("NoSuchAction"), "{msg}");
    }

    #[test]
    fn theme_default_override_and_validation() {
        assert_eq!(
            Config::builtin().theme().unwrap(),
            crate::core::syntax::DEFAULT_THEME
        );
        let cfg = user(r#"theme "Solarized (dark)""#).unwrap();
        assert_eq!(cfg.theme().unwrap(), "Solarized (dark)");

        let msg = format!(
            "{:#}",
            user(r#"theme "nope""#).err().expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("unknown theme \"nope\""), "{msg}");
        assert!(msg.contains("base16-eighties.dark"), "{msg}");

        let msg = format!("{:#}", user(r#"theme"#).err().expect("expected an error"));
        assert!(msg.contains("missing name"), "{msg}");
    }

    #[test]
    fn icons_default_override_and_validation() {
        assert!(Config::builtin().icons().unwrap());
        assert!(!user(r#"icons "none""#).unwrap().icons().unwrap());
        let msg = format!(
            "{:#}",
            user(r#"icons "emoji""#).err().expect("expected an error")
        );
        assert!(msg.contains("/u/config.kdl"), "{msg}");
        assert!(msg.contains("unknown icons mode \"emoji\""), "{msg}");
        assert!(msg.contains("nerd, none"), "{msg}");
    }

    #[test]
    fn image_preview_default_override_and_validation() {
        use crate::files::domain::image::ImagePreviewMode;
        assert_eq!(
            Config::builtin().image_preview().unwrap(),
            ImagePreviewMode::Auto
        );
        assert_eq!(
            user(r#"image-preview "none""#)
                .unwrap()
                .image_preview()
                .unwrap(),
            ImagePreviewMode::None
        );
        let msg = format!(
            "{:#}",
            user(r#"image-preview "sixel""#)
                .err()
                .expect("expected an error")
        );
        assert!(
            msg.contains("unknown image-preview mode \"sixel\""),
            "{msg}"
        );
        assert!(msg.contains("auto, halfblocks, none"), "{msg}");
    }

    #[test]
    fn empty_user_config_equals_builtin() {
        let cfg = user("").unwrap();
        let a = Config::builtin().git_page().unwrap();
        let b = cfg.git_page().unwrap();
        assert_eq!(a.pane_ids, b.pane_ids);
        assert_eq!(a.bindings, b.bindings);
        assert_eq!(a.layout.tab_panes, b.layout.tab_panes);
        assert_eq!(cfg.theme().unwrap(), Config::builtin().theme().unwrap());
        assert_eq!(a.pane_keys.len(), b.pane_keys.len());
        for (pane, keys) in &a.pane_keys {
            assert_eq!(
                format!("{keys:?}"),
                format!("{:?}", b.pane_keys[pane]),
                "{pane}"
            );
        }
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
