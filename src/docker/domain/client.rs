//! Thin wrappers around the `docker` CLI. Every command here is read-only:
//! `version`, `ps`, `images`, `inspect`, `logs`. Nothing in this module
//! (or the page) starts, stops or removes anything.

use crate::docker::domain::types::*;
use std::io::Read;
use std::process::{Command, Stdio};

/// Run a `docker` command and return its stdout on success.
fn run_docker(args: &[&str], context: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{context}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        return Err(if msg.is_empty() {
            format!("{context}: exit status {}", output.status)
        } else {
            msg.to_string()
        });
    }
    Ok(output.stdout)
}

/// `docker version --format json`: fails when the CLI is missing or the
/// daemon is unreachable.
pub fn check_docker_available() -> Result<(), String> {
    run_docker(&["version", "--format", "json"], "docker not found").map(|_| ())
}

pub fn list_containers() -> Result<Vec<Container>, String> {
    let out = run_docker(&["ps", "-a", "--format", "{{json .}}"], "docker ps failed")?;
    parse_json_lines(&String::from_utf8_lossy(&out))
}

pub fn list_images() -> Result<Vec<Image>, String> {
    let out = run_docker(
        &["images", "--format", "{{json .}}"],
        "docker images failed",
    )?;
    parse_json_lines(&String::from_utf8_lossy(&out))
}

pub fn inspect_container(id: &str) -> Result<ContainerInspect, String> {
    let out = run_docker(
        &["inspect", "--type", "container", id],
        "docker inspect failed",
    )?;
    parse_inspect(&String::from_utf8_lossy(&out))
}

pub fn inspect_image(id: &str) -> Result<ImageInspect, String> {
    let out = run_docker(&["inspect", "--type", "image", id], "docker inspect failed")?;
    parse_inspect(&String::from_utf8_lossy(&out))
}

/// `docker logs --timestamps` for `id`. `tail` limits the initial fetch;
/// `since` (an RFC 3339 timestamp from a previous line) fetches only what
/// was written from that instant on. stdout and stderr are read separately
/// and interleaved by timestamp, ANSI escapes stripped.
pub fn fetch_logs(id: &str, tail: usize, since: Option<&str>) -> Result<Vec<String>, String> {
    let tail_s = tail.to_string();
    let mut args = vec!["logs", "--timestamps", "--tail", tail_s.as_str()];
    if let Some(since) = since {
        args.push("--since");
        args.push(since);
    }
    args.push(id);
    let mut child = Command::new("docker")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("docker logs failed: {e}"))?;
    let mut stderr_pipe = child.stderr.take().expect("stderr piped");
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });
    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut stdout);
    }
    let status = child
        .wait()
        .map_err(|e| format!("docker logs failed: {e}"))?;
    let stderr = stderr_thread.join().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout);
    let stderr = String::from_utf8_lossy(&stderr);
    if !status.success() {
        let msg = stderr.lines().last().unwrap_or("").trim();
        return Err(if msg.is_empty() {
            format!("docker logs failed: exit status {status}")
        } else {
            msg.to_string()
        });
    }
    Ok(merge_log_streams(&stdout, &stderr))
}

// === Parsing (pure, unit-tested) ===

/// Parse one JSON object per line (`--format '{{json .}}'`).
pub fn parse_json_lines<T: serde::de::DeserializeOwned>(text: &str) -> Result<Vec<T>, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<T>(l).map_err(|e| format!("JSON parse error: {e}")))
        .collect()
}

/// `docker inspect` prints a one-element array.
pub fn parse_inspect<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    let mut items: Vec<T> =
        serde_json::from_str(text).map_err(|e| format!("JSON parse error: {e}"))?;
    items
        .pop()
        .ok_or_else(|| "docker inspect: no such object".to_string())
}

/// Split a `--timestamps` log line into its RFC 3339 prefix and the message.
pub fn split_timestamp(line: &str) -> (Option<&str>, &str) {
    match line.split_once(' ') {
        Some((ts, rest)) if looks_like_timestamp(ts) => (Some(ts), rest),
        _ => (None, line),
    }
}

fn looks_like_timestamp(s: &str) -> bool {
    s.len() >= 20
        && s.as_bytes()[..4].iter().all(u8::is_ascii_digit)
        && s.as_bytes()[4] == b'-'
        && s.contains('T')
}

/// Interleave the two `docker logs` streams by their timestamp prefix
/// (stable, so lines with equal timestamps keep stream order: stdout first),
/// stripping ANSI escapes and carriage returns.
pub fn merge_log_streams(stdout: &str, stderr: &str) -> Vec<String> {
    let clean = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| strip_ansi(l.trim_end_matches('\r')))
            .filter(|l| !l.is_empty())
            .collect()
    };
    let a = clean(stdout);
    let b = clean(stderr);
    let key = |l: &str| split_timestamp(l).0.unwrap_or("").to_string();
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(a.len() + b.len());
    while i < a.len() && j < b.len() {
        if key(&b[j]) < key(&a[i]) {
            out.push(b[j].clone());
            j += 1;
        } else {
            out.push(a[i].clone());
            i += 1;
        }
    }
    out.extend(a[i..].iter().cloned());
    out.extend(b[j..].iter().cloned());
    out
}

/// Remove ANSI escape sequences (CSI, OSC and two-byte ESC sequences).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: ESC [ params… final byte in 0x40..=0x7E
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: ESC ] … BEL or ESC \
            Some(']') => {
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if c == '\u{7}' || (prev == '\u{1b}' && c == '\\') {
                        break;
                    }
                    prev = c;
                }
            }
            // Two-byte sequences (ESC c, ESC 7, …): drop the byte.
            Some(_) | None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS_LINES: &str = r#"{"Command":"\"docker-entrypoint.s…\"","CreatedAt":"2026-08-04 18:35:29 +0900 JST","ID":"248aed5717b0","Image":"postgres:16","Labels":"com.docker.compose.project=demo,com.docker.compose.service=db","Names":"demo-db-1","Networks":"demo_default","Ports":"127.0.0.1:5432->5432/tcp","RunningFor":"3 weeks ago","Size":"63B","State":"running","Status":"Up 47 hours"}
{"Command":"\"nginx -g 'daemon of…\"","CreatedAt":"2026-08-27 10:00:00 +0900 JST","ID":"17ea558c6618","Image":"nginx:alpine","Labels":"","Names":"web","Networks":"bridge","Ports":"","RunningFor":"1 hour ago","Size":"0B","State":"exited","Status":"Exited (0) 5 minutes ago"}
"#;

    #[test]
    fn parses_docker_ps_json_lines() {
        let rows: Vec<Container> = parse_json_lines(PS_LINES).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "248aed5717b0");
        assert_eq!(rows[0].name, "demo-db-1");
        assert_eq!(rows[0].compose_project(), Some("demo"));
        assert_eq!(rows[0].labels["com.docker.compose.service"], "db");
        assert_eq!(rows[0].ports, "127.0.0.1:5432->5432/tcp");
        assert_eq!(rows[0].state_kind(), ContainerState::Running);
        assert_eq!(rows[1].compose_project(), None);
        assert_eq!(rows[1].state_kind(), ContainerState::Exited);
        assert!(parse_json_lines::<Container>("").unwrap().is_empty());
        assert!(parse_json_lines::<Container>("{not json").is_err());
    }

    #[test]
    fn parses_docker_images_json_lines() {
        let text = r#"{"Containers":"N/A","CreatedAt":"2026-08-20 05:10:55 +0900 JST","CreatedSince":"8 days ago","Digest":"<none>","ID":"c961b5309720","Repository":"nginx","SharedSize":"N/A","Size":"62.3MB","Tag":"alpine","UniqueSize":"N/A"}
{"Containers":"N/A","CreatedAt":"2026-08-01 05:45:37 +0900 JST","CreatedSince":"3 weeks ago","Digest":"<none>","ID":"0560a88f379e","Repository":"<none>","Size":"101MB","Tag":"<none>"}
"#;
        let rows: Vec<Image> = parse_json_lines(text).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name(), "nginx:alpine");
        assert_eq!(rows[0].size, "62.3MB");
        assert_eq!(rows[0].created_since, "8 days ago");
        assert!(!rows[0].is_dangling());
        assert!(rows[1].is_dangling());
    }

    const INSPECT_CONTAINER: &str = r#"[{
  "Id": "248aed5717b0deadbeef",
  "Created": "2026-08-04T09:35:29.123456789Z",
  "State": {"Status": "running", "Running": true, "ExitCode": 0, "Error": "",
            "StartedAt": "2026-08-25T10:00:00Z", "FinishedAt": "0001-01-01T00:00:00Z",
            "Health": {"Status": "healthy", "FailingStreak": 0, "Log": []}},
  "Name": "/demo-db-1",
  "HostConfig": {"RestartPolicy": {"Name": "unless-stopped", "MaximumRetryCount": 0}},
  "Mounts": [{"Type": "volume", "Name": "demo_pgdata", "Source": "/var/lib/docker/volumes/demo_pgdata/_data",
              "Destination": "/var/lib/postgresql/data", "Mode": "z", "RW": true}],
  "Config": {"Hostname": "248aed5717b0", "Image": "postgres:16",
             "Env": ["POSTGRES_PASSWORD=hunter2", "PATH=/usr/bin"],
             "Cmd": ["postgres"], "Entrypoint": ["docker-entrypoint.sh"],
             "Labels": {"com.docker.compose.project": "demo", "com.docker.compose.service": "db"}},
  "NetworkSettings": {"Ports": {"5432/tcp": [{"HostIp": "127.0.0.1", "HostPort": "5432"}], "8080/tcp": null},
                      "Networks": {"demo_default": {"IPAddress": "172.18.0.2", "Gateway": "172.18.0.1"}}}
}]"#;

    #[test]
    fn parses_container_inspect_without_env() {
        let c: ContainerInspect = parse_inspect(INSPECT_CONTAINER).unwrap();
        assert_eq!(c.name, "/demo-db-1");
        assert_eq!(c.state.status, "running");
        assert_eq!(c.state.health.as_ref().unwrap().status, "healthy");
        assert_eq!(c.host_config.restart_policy.name, "unless-stopped");
        assert_eq!(c.mounts.len(), 1);
        assert_eq!(c.mounts[0].destination, "/var/lib/postgresql/data");
        assert_eq!(c.config.cmd.as_deref(), Some(&["postgres".to_string()][..]));
        let ports = c.network_settings.ports.as_ref().unwrap();
        assert_eq!(ports["5432/tcp"].as_ref().unwrap()[0].host_port, "5432");
        assert!(ports["8080/tcp"].is_none());
        let nets = c.network_settings.networks.as_ref().unwrap();
        assert_eq!(nets["demo_default"].ip_address, "172.18.0.2");
        // Environment variables never make it into the struct.
        let dump = format!("{c:?}");
        assert!(!dump.contains("hunter2"));
        assert!(!dump.contains("POSTGRES_PASSWORD"));
    }

    #[test]
    fn parses_image_inspect_without_env() {
        let text = r#"[{
  "Id": "sha256:c961b5309720abcdef",
  "RepoTags": ["nginx:alpine"],
  "RepoDigests": ["nginx@sha256:aaaa"],
  "Created": "2026-08-20T05:10:55Z",
  "Size": 65324032,
  "Architecture": "arm64",
  "Os": "linux",
  "Config": {"Env": ["SECRET=x"], "Cmd": ["nginx", "-g", "daemon off;"], "Entrypoint": ["/docker-entrypoint.sh"],
             "WorkingDir": "", "ExposedPorts": {"80/tcp": {}}, "Labels": {"maintainer": "NGINX Docker Maintainers"}}
}]"#;
        let i: ImageInspect = parse_inspect(text).unwrap();
        assert_eq!(i.repo_tags.as_deref().unwrap(), ["nginx:alpine"]);
        assert_eq!(i.size, 65324032);
        assert_eq!(i.architecture, "arm64");
        assert_eq!(
            i.config
                .exposed_ports
                .as_ref()
                .unwrap()
                .keys()
                .next()
                .unwrap(),
            "80/tcp"
        );
        assert!(!format!("{i:?}").contains("SECRET"));
        assert!(parse_inspect::<ImageInspect>("[]").is_err());
    }

    #[test]
    fn split_timestamp_recognises_rfc3339_prefix() {
        let (ts, rest) = split_timestamp("2026-08-27T09:05:29.121249516Z hello world");
        assert_eq!(ts, Some("2026-08-27T09:05:29.121249516Z"));
        assert_eq!(rest, "hello world");
        assert_eq!(
            split_timestamp("no timestamp here"),
            (None, "no timestamp here")
        );
    }

    #[test]
    fn merges_streams_by_timestamp_and_strips_ansi() {
        let stdout =
            "2026-01-01T00:00:01Z \u{1b}[32mout 1\u{1b}[0m\r\n2026-01-01T00:00:03Z out 3\n";
        let stderr = "2026-01-01T00:00:02Z err 2\n2026-01-01T00:00:03Z err 3\n\n";
        assert_eq!(
            merge_log_streams(stdout, stderr),
            [
                "2026-01-01T00:00:01Z out 1",
                "2026-01-01T00:00:02Z err 2",
                "2026-01-01T00:00:03Z out 3",
                "2026-01-01T00:00:03Z err 3",
            ]
        );
    }

    #[test]
    fn strip_ansi_handles_csi_osc_and_bare_escapes() {
        assert_eq!(strip_ansi("\u{1b}[1;31mred\u{1b}[0m plain"), "red plain");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}text"), "text");
        assert_eq!(strip_ansi("\u{1b}]8;;http://x\u{1b}\\link"), "link");
        assert_eq!(strip_ansi("a\u{1b}cb"), "ab");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
    }
}
