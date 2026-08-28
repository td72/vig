//! Detail pane: an inspect summary for the selected container or image,
//! or a member list for a compose project row. Environment variables are
//! never part of the data (see `domain::types`), so nothing here can show them.

use crate::core::app::AppContext;
use crate::core::keymap::{half_page_step, nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use crate::docker::domain::client;
use crate::docker::domain::types::{
    Container, ContainerInspect, Image, ImageInspect, InspectSummary,
};
use crate::docker::state::DockerBgMessage;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub enum DetailAction {
    Nav(NavAction),
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(DetailAction, nav: Nav, Back, Esc);

impl ActionHelp for DetailAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DetailAction::Nav(NavAction::MoveDown) => Some("Scroll down"),
            DetailAction::Nav(NavAction::MoveUp) => Some("Scroll up"),
            DetailAction::Nav(nav) => nav.label(),
            DetailAction::Back => Some("Back to list"),
            DetailAction::Esc => Some("Back to list"),
        }
    }
}

pub fn default_keymap() -> Keymap<DetailAction> {
    Keymap::new()
        .bindings(nav_bindings(DetailAction::Nav))
        .key(KeyCode::Char('h'), DetailAction::Back)
        .key(KeyCode::Esc, DetailAction::Esc)
}

/// What to inspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailTarget {
    Container { id: String, name: String },
    Image { id: String, name: String },
}

impl DetailTarget {
    pub fn container(c: &Container) -> Self {
        Self::Container {
            id: c.id.clone(),
            name: c.name.clone(),
        }
    }

    pub fn image(i: &Image) -> Self {
        Self::Image {
            id: i.id.clone(),
            name: i.name(),
        }
    }

    /// Cache key / message correlation id.
    pub fn key(&self) -> String {
        match self {
            Self::Container { id, .. } => format!("c:{id}"),
            Self::Image { id, .. } => format!("i:{id}"),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Container { name, .. } => format!("container {name}"),
            Self::Image { name, .. } => format!("image {name}"),
        }
    }

    fn fetch(&self) -> Result<InspectSummary, String> {
        match self {
            Self::Container { id, .. } => {
                client::inspect_container(id).map(|c| InspectSummary::Container(Box::new(c)))
            }
            Self::Image { id, .. } => {
                client::inspect_image(id).map(|i| InspectSummary::Image(Box::new(i)))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum DetailContent {
    None,
    Loading(String),
    Inspect(InspectSummary),
    /// Compose project row: member containers as (name, state, status).
    Project {
        name: String,
        members: Vec<(String, String, String)>,
    },
    Error(String),
}

pub struct DetailPane {
    pane_id: usize,
    keymap: Keymap<DetailAction>,
    pub content: DetailContent,
    target: Option<DetailTarget>,
    cache: HashMap<String, InspectSummary>,
    scroll: usize,
    view_height: u16,
    line_count: usize,
}

impl DetailPane {
    pub fn new(pane_id: usize) -> Self {
        Self {
            pane_id,
            keymap: default_keymap(),
            content: DetailContent::None,
            target: None,
            cache: HashMap::new(),
            scroll: 0,
            view_height: 20,
            line_count: 0,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<DetailAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<DetailAction> {
        &self.keymap
    }

    pub fn show_none(&mut self) {
        self.target = None;
        self.content = DetailContent::None;
        self.scroll = 0;
    }

    pub fn show_project(&mut self, name: &str, members: &[&Container]) {
        self.target = None;
        self.content = DetailContent::Project {
            name: name.to_string(),
            members: members
                .iter()
                .map(|c| (c.name.clone(), c.state.clone(), c.status.clone()))
                .collect(),
        };
        self.scroll = 0;
    }

    /// Show `target`: from the cache when possible, otherwise fetch in the
    /// background. Re-selecting the current target is a no-op.
    pub fn load(&mut self, target: DetailTarget, tx: &mpsc::Sender<DockerBgMessage>) {
        if self.target.as_ref() == Some(&target) {
            return;
        }
        let key = target.key();
        self.scroll = 0;
        if let Some(cached) = self.cache.get(&key) {
            self.content = DetailContent::Inspect(cached.clone());
            self.target = Some(target);
            return;
        }
        self.content = DetailContent::Loading(target.label());
        self.spawn_fetch(&target, tx);
        self.target = Some(target);
    }

    /// Re-fetch the current target without dropping what is on screen
    /// (periodic refresh: state / health / ports change while running).
    pub fn reload_current(&mut self, tx: &mpsc::Sender<DockerBgMessage>) {
        if let Some(target) = self.target.clone() {
            self.spawn_fetch(&target, tx);
        }
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    fn spawn_fetch(&self, target: &DetailTarget, tx: &mpsc::Sender<DockerBgMessage>) {
        let tx = tx.clone();
        let target = target.clone();
        std::thread::spawn(move || {
            let key = target.key();
            let result = target.fetch();
            let _ = tx.send(DockerBgMessage::Inspect { key, result });
        });
    }

    /// Apply a fetch result; stale results (target changed meanwhile) only
    /// warm the cache.
    pub fn apply(&mut self, key: &str, result: Result<InspectSummary, String>) {
        let current = self.target.as_ref().map(DetailTarget::key);
        match result {
            Ok(summary) => {
                self.cache.insert(key.to_string(), summary.clone());
                if current.as_deref() == Some(key) {
                    self.content = DetailContent::Inspect(summary);
                }
            }
            Err(e) => {
                if current.as_deref() == Some(key)
                    && !matches!(self.content, DetailContent::Inspect(_))
                {
                    self.content = DetailContent::Error(e);
                }
            }
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: DetailAction) -> Vec<PaneEvent> {
        match action {
            DetailAction::Nav(nav) => {
                let max = self.line_count.saturating_sub(self.view_height as usize);
                let half = half_page_step(self.view_height) as usize;
                self.scroll = match nav {
                    NavAction::MoveDown => self.scroll + 1,
                    NavAction::MoveUp => self.scroll.saturating_sub(1),
                    NavAction::HalfPageDown => self.scroll + half,
                    NavAction::HalfPageUp => self.scroll.saturating_sub(half),
                    NavAction::JumpTop => 0,
                    NavAction::JumpBottom => max,
                }
                .min(max);
                vec![]
            }
            DetailAction::Back | DetailAction::Esc => {
                vec![PaneEvent::SetFocus(shared.previous_pane)]
            }
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        match &self.content {
            DetailContent::None => vec![Line::from(Span::styled(
                "  Select a container or image",
                dim,
            ))],
            DetailContent::Loading(label) => {
                vec![Line::from(Span::styled(
                    format!("  Loading {label}..."),
                    dim,
                ))]
            }
            DetailContent::Error(e) => vec![Line::from(Span::styled(
                format!("  Error: {e}"),
                Style::default().fg(Color::Red),
            ))],
            DetailContent::Project { name, members } => project_lines(name, members),
            DetailContent::Inspect(InspectSummary::Container(c)) => container_lines(c),
            DetailContent::Inspect(InspectSummary::Image(i)) => image_lines(i),
        }
    }
}

impl Pane<PaneEvent> for DetailPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let lines = self.lines();
        self.line_count = lines.len();
        self.scroll = self
            .scroll
            .min(self.line_count.saturating_sub(self.view_height as usize));
        let block = theme::pane_block("Detail", shared.focused_pane == self.pane_id);
        let para = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll as u16, 0));
        f.render_widget(para, area);
    }
}

// === Line builders ===

const KEY_WIDTH: usize = 13;

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {title}"),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

fn kv(key: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<KEY_WIDTH$} "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(value.into()),
    ])
}

fn item(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::raw(format!("  {}", text.into())))
}

fn badge(text: &str, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(Color::Black).bg(bg),
    )
}

fn state_color(status: &str) -> Color {
    match status {
        "running" => Color::Green,
        "paused" | "restarting" => Color::Yellow,
        "exited" | "dead" => Color::Red,
        _ => Color::Gray,
    }
}

/// `2026-08-04T09:35:29.123456789Z` → `2026-08-04 09:35:29 UTC`; Docker's
/// zero time (`0001-01-01…`) and empty strings become `-`.
pub fn fmt_time(ts: &str) -> String {
    if ts.is_empty() || ts.starts_with("0001-01-01") {
        return "-".to_string();
    }
    let s: String = ts.chars().take(19).collect();
    match s.split_once('T') {
        Some((d, t)) => format!("{d} {t} UTC"),
        None => s,
    }
}

/// `nginx@sha256:db35bfc6b295…` — the full digest is too wide for the pane.
fn short_digest(digest: &str) -> String {
    match digest.split_once("@sha256:") {
        Some((repo, hash)) => format!(
            "{repo}@sha256:{}…",
            hash.chars().take(12).collect::<String>()
        ),
        None => digest.to_string(),
    }
}

fn short_id(id: &str) -> String {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    id.chars().take(12).collect()
}

fn join_cmd(cmd: &Option<Vec<String>>) -> String {
    match cmd {
        Some(v) if !v.is_empty() => v.join(" "),
        _ => "-".to_string(),
    }
}

pub fn human_size(bytes: u64) -> String {
    crate::files::domain::fs::human_size(bytes)
}

pub fn container_lines(c: &ContainerInspect) -> Vec<Line<'static>> {
    let name = c.name.strip_prefix('/').unwrap_or(&c.name).to_string();
    let mut title = vec![
        Span::raw(" "),
        Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        badge(&c.state.status, state_color(&c.state.status)),
    ];
    if let Some(h) = &c.state.health {
        if !h.status.is_empty() {
            let color = match h.status.as_str() {
                "healthy" => Color::Green,
                "unhealthy" => Color::Red,
                _ => Color::Yellow,
            };
            title.push(Span::raw(" "));
            title.push(badge(&h.status, color));
        }
    }
    let mut lines = vec![Line::from(title), Line::default()];

    lines.push(section("General"));
    lines.push(kv("Id", short_id(&c.id)));
    lines.push(kv("Image", c.config.image.clone()));
    lines.push(kv("Created", fmt_time(&c.created)));
    let rp = &c.host_config.restart_policy;
    let policy = match (rp.name.as_str(), rp.maximum_retry_count) {
        ("", _) => "no".to_string(),
        (n, 0) => n.to_string(),
        (n, max) => format!("{n} (max {max})"),
    };
    lines.push(kv("Restart", policy));

    lines.push(Line::default());
    lines.push(section("State"));
    lines.push(kv("Status", c.state.status.clone()));
    lines.push(kv("Started", fmt_time(&c.state.started_at)));
    lines.push(kv("Finished", fmt_time(&c.state.finished_at)));
    lines.push(kv("Exit code", c.state.exit_code.to_string()));
    if !c.state.error.is_empty() {
        lines.push(kv("Error", c.state.error.clone()));
    }
    if let Some(h) = &c.state.health {
        lines.push(kv(
            "Health",
            format!("{} (failing streak {})", h.status, h.failing_streak),
        ));
    }

    lines.push(Line::default());
    lines.push(section("Command"));
    lines.push(kv("Entrypoint", join_cmd(&c.config.entrypoint)));
    lines.push(kv("Cmd", join_cmd(&c.config.cmd)));

    lines.push(Line::default());
    lines.push(section(&format!("Mounts ({})", c.mounts.len())));
    for m in &c.mounts {
        let source = match (&m.name, m.kind.as_str()) {
            (Some(n), "volume") => format!("volume {n}"),
            _ => m.source.clone(),
        };
        let mode = if m.mode.is_empty() {
            if m.rw { "rw" } else { "ro" }.to_string()
        } else {
            m.mode.clone()
        };
        lines.push(item(format!("{source} → {}  ({mode})", m.destination)));
    }

    lines.push(Line::default());
    let empty_ports = Default::default();
    let ports = c.network_settings.ports.as_ref().unwrap_or(&empty_ports);
    lines.push(section(&format!("Ports ({})", ports.len())));
    for (port, bindings) in ports {
        match bindings {
            Some(b) if !b.is_empty() => {
                for pb in b {
                    lines.push(item(format!("{}:{} → {port}", pb.host_ip, pb.host_port)));
                }
            }
            _ => lines.push(item(format!("{port} (not published)"))),
        }
    }

    lines.push(Line::default());
    let empty_networks = Default::default();
    let networks = c
        .network_settings
        .networks
        .as_ref()
        .unwrap_or(&empty_networks);
    lines.push(section(&format!("Networks ({})", networks.len())));
    for (name, n) in networks {
        let ip = if n.ip_address.is_empty() {
            "-".to_string()
        } else {
            n.ip_address.clone()
        };
        let mut text = format!("{name}  {ip}");
        if !n.gateway.is_empty() {
            text.push_str(&format!("  gw {}", n.gateway));
        }
        lines.push(item(text));
    }

    lines.push(Line::default());
    let labels = c.config.labels.clone().unwrap_or_default();
    lines.push(section(&format!("Labels ({})", labels.len())));
    for (k, v) in &labels {
        lines.push(item(format!("{k}={v}")));
    }
    lines
}

pub fn image_lines(i: &ImageInspect) -> Vec<Line<'static>> {
    let tags = i.repo_tags.clone().unwrap_or_default();
    let title = tags.first().cloned().unwrap_or_else(|| short_id(&i.id));
    let mut lines = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            badge("image", Color::Blue),
        ]),
        Line::default(),
        section("General"),
        kv("Id", short_id(&i.id)),
        kv(
            "Tags",
            if tags.is_empty() {
                "<none>".to_string()
            } else {
                tags.join(", ")
            },
        ),
    ];
    let digests = i.repo_digests.clone().unwrap_or_default();
    if !digests.is_empty() {
        let short: Vec<String> = digests.iter().map(|d| short_digest(d)).collect();
        lines.push(kv("Digests", short.join(", ")));
    }
    lines.push(kv("Created", fmt_time(&i.created)));
    lines.push(kv("Size", human_size(i.size)));
    lines.push(kv("Platform", format!("{}/{}", i.os, i.architecture)));

    lines.push(Line::default());
    lines.push(section("Command"));
    lines.push(kv("Entrypoint", join_cmd(&i.config.entrypoint)));
    lines.push(kv("Cmd", join_cmd(&i.config.cmd)));
    if !i.config.working_dir.is_empty() {
        lines.push(kv("WorkingDir", i.config.working_dir.clone()));
    }

    lines.push(Line::default());
    let ports = i.config.exposed_ports.clone().unwrap_or_default();
    lines.push(section(&format!("Exposed ports ({})", ports.len())));
    for port in ports.keys() {
        lines.push(item(port.clone()));
    }

    lines.push(Line::default());
    let labels = i.config.labels.clone().unwrap_or_default();
    lines.push(section(&format!("Labels ({})", labels.len())));
    for (k, v) in &labels {
        lines.push(item(format!("{k}={v}")));
    }
    lines
}

fn project_lines(name: &str, members: &[(String, String, String)]) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                name.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            badge("compose project", Color::Cyan),
        ]),
        Line::default(),
        section(&format!("Containers ({})", members.len())),
    ];
    for (name, state, status) in members {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{state:<10} "),
                Style::default().fg(state_color(state)),
            ),
            Span::raw(name.clone()),
            Span::styled(format!("  {status}"), Style::default().fg(Color::DarkGray)),
        ]));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_time_trims_nanos_and_hides_zero_time() {
        assert_eq!(
            fmt_time("2026-08-04T09:35:29.123456789Z"),
            "2026-08-04 09:35:29 UTC"
        );
        assert_eq!(fmt_time("0001-01-01T00:00:00Z"), "-");
        assert_eq!(fmt_time(""), "-");
    }

    #[test]
    fn short_id_strips_sha_prefix() {
        assert_eq!(short_id("sha256:c961b5309720abcdef"), "c961b5309720");
        assert_eq!(short_id("abc"), "abc");
        assert_eq!(
            short_digest(
                "nginx@sha256:db35bfc6b2951e7f8a72db5db120288c127ffaeeb4a6d4b95a26fead017d5913"
            ),
            "nginx@sha256:db35bfc6b295…"
        );
        assert_eq!(short_digest("plain"), "plain");
    }

    #[test]
    fn container_lines_render_sections_and_never_env() {
        const JSON: &str = r#"[{"Id":"abcdef123456789","Name":"/web","Created":"2026-08-27T01:00:00Z",
          "State":{"Status":"running","ExitCode":0,"StartedAt":"2026-08-27T01:00:01Z","FinishedAt":"0001-01-01T00:00:00Z"},
          "HostConfig":{"RestartPolicy":{"Name":"on-failure","MaximumRetryCount":3}},
          "Mounts":[{"Type":"bind","Source":"/host","Destination":"/data","Mode":"ro","RW":false}],
          "Config":{"Image":"nginx:alpine","Env":["TOKEN=verysecret"],"Cmd":["nginx"],"Labels":{"a":"b"}},
          "NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}],"443/tcp":null},
                             "Networks":{"bridge":{"IPAddress":"172.17.0.2","Gateway":"172.17.0.1"}}}}]"#;
        let c: ContainerInspect = client::parse_inspect(JSON).unwrap();
        let text: Vec<String> = container_lines(&c)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.to_string()).collect())
            .collect();
        let joined = text.join("\n");
        assert!(joined.contains("web"));
        assert!(joined.contains("on-failure (max 3)"));
        assert!(joined.contains("/host → /data  (ro)"));
        assert!(joined.contains("0.0.0.0:8080 → 80/tcp"));
        assert!(joined.contains("443/tcp (not published)"));
        assert!(joined.contains("bridge  172.17.0.2  gw 172.17.0.1"));
        assert!(joined.contains("a=b"));
        assert!(!joined.contains("verysecret"));
        assert!(!joined.contains("TOKEN"));
    }

    #[test]
    fn detail_pane_load_uses_cache_and_ignores_stale_results() {
        let (tx, rx) = mpsc::channel();
        let mut pane = DetailPane::new(0);
        let a = DetailTarget::Image {
            id: "a".into(),
            name: "a:1".into(),
        };
        let b = DetailTarget::Image {
            id: "b".into(),
            name: "b:1".into(),
        };
        pane.load(a.clone(), &tx);
        assert!(matches!(pane.content, DetailContent::Loading(_)));
        pane.load(b.clone(), &tx);
        // A late result for `a` warms the cache but does not replace `b`.
        let img: ImageInspect =
            client::parse_inspect(r#"[{"Id":"sha256:a","RepoTags":["a:1"]}]"#).unwrap();
        pane.apply(&a.key(), Ok(InspectSummary::Image(Box::new(img))));
        assert!(matches!(pane.content, DetailContent::Loading(_)));
        pane.apply(&b.key(), Err("boom".into()));
        assert!(matches!(pane.content, DetailContent::Error(_)));
        // Going back to `a` is served from the cache without a fetch.
        while rx.try_recv().is_ok() {}
        pane.load(a.clone(), &tx);
        assert!(matches!(
            pane.content,
            DetailContent::Inspect(InspectSummary::Image(_))
        ));
        assert!(rx.try_recv().is_err());
    }
}
