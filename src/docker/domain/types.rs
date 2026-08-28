//! Data shapes read from the `docker` CLI's JSON output.
//!
//! The inspect structs deliberately declare only the fields the page shows.
//! In particular there is no `Env` field anywhere: `docker inspect` returns
//! `Config.Env`, which routinely carries secrets, and serde drops unknown
//! fields, so environment variables are never even deserialized.

use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;

pub const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";

// === docker ps ===

/// One row of `docker ps -a --format '{{json .}}'`.
#[derive(Debug, Clone, Deserialize)]
pub struct Container {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Names")]
    pub name: String,
    #[serde(rename = "Image")]
    pub image: String,
    /// `running`, `exited`, `paused`, `restarting`, `created`, `dead`, …
    #[serde(rename = "State")]
    pub state: String,
    /// Human status text, e.g. `Up 3 hours (healthy)`, `Exited (0) 2 days ago`.
    #[serde(rename = "Status", default)]
    pub status: String,
    #[serde(rename = "Ports", default)]
    pub ports: String,
    #[serde(rename = "Labels", default, deserialize_with = "labels_from_string")]
    pub labels: BTreeMap<String, String>,
}

impl Container {
    pub fn state_kind(&self) -> ContainerState {
        ContainerState::parse(&self.state)
    }

    pub fn compose_project(&self) -> Option<&str> {
        self.labels.get(COMPOSE_PROJECT_LABEL).map(String::as_str)
    }
}

/// Coarse container state, for icons and sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContainerState {
    Running,
    Restarting,
    Paused,
    Created,
    Exited,
    Dead,
    Other,
}

impl ContainerState {
    pub fn parse(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "restarting" => Self::Restarting,
            "paused" => Self::Paused,
            "created" => Self::Created,
            "exited" | "removing" => Self::Exited,
            "dead" => Self::Dead,
            _ => Self::Other,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Running => "●",
            Self::Restarting => "↻",
            Self::Paused => "‖",
            Self::Created => "○",
            Self::Exited => "■",
            Self::Dead => "✗",
            Self::Other => "?",
        }
    }

    /// Running containers sort first; everything else keeps its own order.
    pub fn sort_rank(self) -> u8 {
        match self {
            Self::Running => 0,
            Self::Restarting => 1,
            Self::Paused => 2,
            Self::Created => 3,
            Self::Exited => 4,
            Self::Dead => 5,
            Self::Other => 6,
        }
    }
}

/// `docker ps` flattens labels into `k=v,k=v`. Values may themselves
/// contain commas (OCI description labels do), so a segment without `=`
/// is glued back onto the previous value.
fn labels_from_string<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    let raw = String::deserialize(d)?;
    Ok(parse_label_string(&raw))
}

pub fn parse_label_string(raw: &str) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let mut last_key: Option<String> = None;
    for seg in raw.split(',') {
        match seg.split_once('=') {
            Some((k, v)) if !k.is_empty() && !k.contains(' ') => {
                labels.insert(k.to_string(), v.to_string());
                last_key = Some(k.to_string());
            }
            _ => {
                if let Some(k) = &last_key {
                    if let Some(v) = labels.get_mut(k) {
                        v.push(',');
                        v.push_str(seg);
                    }
                }
            }
        }
    }
    labels
}

// === docker images ===

/// One row of `docker images --format '{{json .}}'`.
#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Repository")]
    pub repository: String,
    #[serde(rename = "Tag")]
    pub tag: String,
    #[serde(rename = "Size", default)]
    pub size: String,
    /// Relative age, e.g. `3 weeks ago`.
    #[serde(rename = "CreatedSince", default)]
    pub created_since: String,
}

impl Image {
    /// `<none>` repository or tag: an untagged intermediate / leftover layer.
    pub fn is_dangling(&self) -> bool {
        self.repository == "<none>" || self.tag == "<none>"
    }

    pub fn name(&self) -> String {
        format!("{}:{}", self.repository, self.tag)
    }
}

// === docker inspect (container) ===

/// The subset of `docker inspect <container>` shown in the detail pane.
#[derive(Debug, Clone, Deserialize)]
pub struct ContainerInspect {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Created", default)]
    pub created: String,
    #[serde(rename = "State", default)]
    pub state: InspectState,
    #[serde(rename = "HostConfig", default)]
    pub host_config: HostConfig,
    #[serde(rename = "Mounts", default)]
    pub mounts: Vec<Mount>,
    #[serde(rename = "Config", default)]
    pub config: ContainerConfig,
    #[serde(rename = "NetworkSettings", default)]
    pub network_settings: NetworkSettings,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InspectState {
    #[serde(rename = "Status", default)]
    pub status: String,
    #[serde(rename = "ExitCode", default)]
    pub exit_code: i64,
    #[serde(rename = "Error", default)]
    pub error: String,
    #[serde(rename = "StartedAt", default)]
    pub started_at: String,
    #[serde(rename = "FinishedAt", default)]
    pub finished_at: String,
    #[serde(rename = "Health", default)]
    pub health: Option<Health>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Health {
    #[serde(rename = "Status", default)]
    pub status: String,
    #[serde(rename = "FailingStreak", default)]
    pub failing_streak: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HostConfig {
    #[serde(rename = "RestartPolicy", default)]
    pub restart_policy: RestartPolicy,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RestartPolicy {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "MaximumRetryCount", default)]
    pub maximum_retry_count: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Mount {
    #[serde(rename = "Type", default)]
    pub kind: String,
    #[serde(rename = "Name", default)]
    pub name: Option<String>,
    #[serde(rename = "Source", default)]
    pub source: String,
    #[serde(rename = "Destination", default)]
    pub destination: String,
    #[serde(rename = "Mode", default)]
    pub mode: String,
    #[serde(rename = "RW", default)]
    pub rw: bool,
}

/// `Config` of a container. **No `Env` field on purpose** (see module docs).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ContainerConfig {
    #[serde(rename = "Image", default)]
    pub image: String,
    #[serde(rename = "Cmd", default)]
    pub cmd: Option<Vec<String>>,
    #[serde(rename = "Entrypoint", default)]
    pub entrypoint: Option<Vec<String>>,
    #[serde(rename = "Labels", default)]
    pub labels: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkSettings {
    /// `"5432/tcp": [{HostIp, HostPort}]`, or `null` when not published.
    #[serde(rename = "Ports", default)]
    pub ports: Option<BTreeMap<String, Option<Vec<PortBinding>>>>,
    #[serde(rename = "Networks", default)]
    pub networks: Option<BTreeMap<String, Network>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PortBinding {
    #[serde(rename = "HostIp", default)]
    pub host_ip: String,
    #[serde(rename = "HostPort", default)]
    pub host_port: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Network {
    #[serde(rename = "IPAddress", default)]
    pub ip_address: String,
    #[serde(rename = "Gateway", default)]
    pub gateway: String,
}

// === docker inspect (image) ===

/// The subset of `docker inspect <image>` shown in the detail pane.
#[derive(Debug, Clone, Deserialize)]
pub struct ImageInspect {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "RepoTags", default)]
    pub repo_tags: Option<Vec<String>>,
    #[serde(rename = "RepoDigests", default)]
    pub repo_digests: Option<Vec<String>>,
    #[serde(rename = "Created", default)]
    pub created: String,
    #[serde(rename = "Size", default)]
    pub size: u64,
    #[serde(rename = "Architecture", default)]
    pub architecture: String,
    #[serde(rename = "Os", default)]
    pub os: String,
    #[serde(rename = "Config", default)]
    pub config: ImageConfig,
}

/// `Config` of an image. **No `Env` field on purpose** (see module docs).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImageConfig {
    #[serde(rename = "Cmd", default)]
    pub cmd: Option<Vec<String>>,
    #[serde(rename = "Entrypoint", default)]
    pub entrypoint: Option<Vec<String>>,
    #[serde(rename = "WorkingDir", default)]
    pub working_dir: String,
    #[serde(rename = "ExposedPorts", default)]
    pub exposed_ports: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(rename = "Labels", default)]
    pub labels: Option<BTreeMap<String, String>>,
}

/// What the detail pane can show for a selected list row.
#[derive(Debug, Clone)]
pub enum InspectSummary {
    Container(Box<ContainerInspect>),
    Image(Box<ImageInspect>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_string_keeps_commas_inside_values() {
        let labels = parse_label_string(
            "com.docker.compose.project=demo,org.opencontainers.image.description=One Postgres, many uses,com.docker.compose.service=db",
        );
        assert_eq!(labels[COMPOSE_PROJECT_LABEL], "demo");
        assert_eq!(labels["com.docker.compose.service"], "db");
        assert_eq!(
            labels["org.opencontainers.image.description"],
            "One Postgres, many uses"
        );
        assert!(parse_label_string("").is_empty());
    }

    #[test]
    fn container_state_parses_and_ranks() {
        assert_eq!(ContainerState::parse("running"), ContainerState::Running);
        assert_eq!(ContainerState::parse("exited"), ContainerState::Exited);
        assert_eq!(ContainerState::parse("weird"), ContainerState::Other);
        assert!(ContainerState::Running.sort_rank() < ContainerState::Exited.sort_rank());
    }

    #[test]
    fn image_dangling_and_name() {
        let img = Image {
            id: "abc".into(),
            repository: "<none>".into(),
            tag: "<none>".into(),
            size: "1MB".into(),
            created_since: String::new(),
        };
        assert!(img.is_dangling());
        let img = Image {
            repository: "nginx".into(),
            tag: "alpine".into(),
            ..img
        };
        assert!(!img.is_dangling());
        assert_eq!(img.name(), "nginx:alpine");
    }
}
