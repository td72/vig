//! Listening sockets. macOS asks `lsof`, Linux asks `ss`; both outputs are
//! parsed from text so the parsers can be tested against fixtures. Rows the
//! current user may not inspect keep their port but lose the owner
//! (`pid: None`, rendered as `(no access)`).

use crate::procs::domain::types::{PortEntry, Proto};
use std::process::Command;

/// Run the platform tool and parse its output. `Err` carries a one-line
/// notice for the pane (tool missing, unsupported platform, …).
pub fn fetch_ports() -> Result<Vec<PortEntry>, String> {
    let mut entries = if cfg!(target_os = "macos") {
        parse_lsof(&run("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-iUDP"])?)
    } else if cfg!(target_os = "linux") {
        parse_ss(&run("ss", &["-tulnp"])?)
    } else {
        return Err("listening ports are not supported on this platform".to_string());
    };
    entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    entries.dedup();
    Ok(entries)
}

/// Run `program` and return its stdout. A non-zero exit with output is
/// still accepted: `lsof` exits 1 when some sockets could not be inspected
/// but prints the rest.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program).args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("`{program}` not found; install it to list listening ports")
        } else {
            format!("`{program}` failed: {e}")
        }
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() && stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.lines().next().unwrap_or("").trim();
        return Err(if reason.is_empty() {
            format!("`{program}` exited with {}", output.status)
        } else {
            format!("`{program}`: {reason}")
        });
    }
    Ok(stdout)
}

/// Split `host:port` at the last colon. Returns `None` for wildcard or
/// non-numeric ports (`*:*`, `*:mdns`).
fn split_host_port(s: &str) -> Option<(&str, u16)> {
    let (host, port) = s.rsplit_once(':')?;
    let port = port.parse().ok()?;
    Some((host, port))
}

/// Parse `lsof -nP -iTCP -sTCP:LISTEN -iUDP` output:
///
/// ```text
/// COMMAND     PID     USER   FD   TYPE  DEVICE SIZE/OFF NODE NAME
/// rapportd   1035 hosokawa   10u  IPv4  0x8ec1      0t0  TCP *:57094 (LISTEN)
/// Browser    1576 hosokawa   57u  IPv4  0xf3df      0t0  UDP *:5353
/// ```
///
/// Columns are read from the right because `COMMAND` may contain spaces.
/// Connected UDP sockets (`a:1->b:2`) and unbound ones (`*:*`) are skipped.
pub fn parse_lsof(out: &str) -> Vec<PortEntry> {
    let mut entries = Vec::new();
    for line in out.lines().skip_while(|l| l.starts_with("COMMAND")) {
        let mut cols: Vec<&str> = line.split_whitespace().collect();
        if cols.last() == Some(&"(LISTEN)") {
            cols.pop();
        }
        // COMMAND… PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
        if cols.len() < 9 {
            continue;
        }
        let name = cols[cols.len() - 1];
        let proto = match cols[cols.len() - 2] {
            "TCP" => Proto::Tcp,
            "UDP" => Proto::Udp,
            _ => continue,
        };
        if name.contains("->") {
            continue;
        }
        let Some((addr, port)) = split_host_port(name) else {
            continue;
        };
        let pid = cols[cols.len() - 8].parse::<u32>().ok();
        // lsof writes spaces inside a command name as `\x20`.
        let command = cols[..cols.len() - 8].join(" ").replace("\\x20", " ");
        entries.push(PortEntry {
            proto,
            addr: addr.to_string(),
            port,
            pid,
            name: (!command.is_empty()).then_some(command),
        });
    }
    entries
}

/// Parse `ss -tulnp` output:
///
/// ```text
/// Netid State  Recv-Q Send-Q Local Address:Port Peer Address:Port Process
/// tcp   LISTEN 0      128    0.0.0.0:22         0.0.0.0:*         users:(("sshd",pid=1234,fd=3))
/// udp   UNCONN 0      0      127.0.0.53%lo:53   0.0.0.0:*
/// ```
///
/// A missing `users:` column means the socket belongs to another user.
pub fn parse_ss(out: &str) -> Vec<PortEntry> {
    let mut entries = Vec::new();
    for line in out.lines().skip_while(|l| l.starts_with("Netid")) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        let proto = match cols[0] {
            "tcp" => Proto::Tcp,
            "udp" => Proto::Udp,
            _ => continue,
        };
        let Some((addr, port)) = split_host_port(cols[4]) else {
            continue;
        };
        let (pid, name) = cols
            .get(6)
            .and_then(|p| parse_ss_process(p))
            .map(|(n, p)| (Some(p), Some(n)))
            .unwrap_or((None, None));
        entries.push(PortEntry {
            proto,
            addr: addr.to_string(),
            port,
            pid,
            name,
        });
    }
    entries
}

/// `users:(("sshd",pid=1234,fd=3),("sshd",pid=1235,fd=3))` → `("sshd", 1234)`.
fn parse_ss_process(field: &str) -> Option<(String, u32)> {
    let rest = field.strip_prefix("users:((")?;
    let name = rest.strip_prefix('"')?.split('"').next()?.to_string();
    let pid = rest
        .split("pid=")
        .nth(1)?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((name, pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSOF: &str = "\
COMMAND     PID     USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
rapportd   1035 hosokawa   10u  IPv4 0x8ec139173cf4c885      0t0  TCP *:57094 (LISTEN)
rapportd   1035 hosokawa   11u  IPv6 0x1596ef120bcd6954      0t0  TCP *:57094 (LISTEN)
identitys  1048 hosokawa   14u  IPv4 0xd8493a92f0993c04      0t0  UDP *:*
zed        1392 hosokawa   16u  IPv4 0xffd32dd05e333b73      0t0  TCP 127.0.0.1:44438 (LISTEN)
Browser    1576 hosokawa   28u  IPv6 0x7269427e6fe1b218      0t0  UDP [240d::1]:64824->[2404::5f]:443
Browser    1576 hosokawa   57u  IPv4 0xf3df3cf5ae5c846d      0t0  UDP *:5353
Code Help  2001 hosokawa   20u  IPv6 0x0000000000000001      0t0  TCP [::1]:8080 (LISTEN)
Adobe\\x20 2179 hosokawa   30u  IPv4 0x0000000000000002      0t0  TCP 127.0.0.1:15292 (LISTEN)
";

    #[test]
    fn lsof_parses_listening_sockets_and_skips_the_rest() {
        let e = parse_lsof(LSOF);
        let rows: Vec<String> = e
            .iter()
            .map(|p| {
                format!(
                    "{} {} {} {}",
                    p.proto.label(),
                    p.address(),
                    p.pid.map_or("-".to_string(), |p| p.to_string()),
                    p.name.as_deref().unwrap_or("-")
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                "tcp *:57094 1035 rapportd",
                "tcp *:57094 1035 rapportd",
                "tcp 127.0.0.1:44438 1392 zed",
                "udp *:5353 1576 Browser",
                "tcp [::1]:8080 2001 Code Help",
                "tcp 127.0.0.1:15292 2179 Adobe ",
            ]
        );
    }

    #[test]
    fn lsof_row_without_numeric_pid_is_kept_as_no_access() {
        // Same shape as a row whose owner is hidden from us.
        let out = "COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\n\
                   ?  ?  ?  4u IPv4 0x1 0t0 TCP *:22 (LISTEN)\n";
        let e = parse_lsof(out);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].pid, None);
        assert_eq!(e[0].port, 22);
    }

    const SS: &str = "\
Netid State  Recv-Q Send-Q         Local Address:Port  Peer Address:Port Process
tcp   LISTEN 0      128                  0.0.0.0:22         0.0.0.0:*     users:((\"sshd\",pid=1234,fd=3),(\"sshd\",pid=1235,fd=3))
udp   UNCONN 0      0              127.0.0.53%lo:53         0.0.0.0:*
tcp   LISTEN 0      511                     [::]:80            [::]:*     users:((\"nginx\",pid=800,fd=6))
udp   UNCONN 0      0                          *:5353             *:*     users:((\"avahi-daemon\",pid=612,fd=12))
";

    #[test]
    fn ss_parses_rows_and_marks_hidden_owners() {
        let e = parse_ss(SS);
        assert_eq!(e.len(), 4);
        assert_eq!(e[0].proto, Proto::Tcp);
        assert_eq!(e[0].address(), "0.0.0.0:22");
        assert_eq!(e[0].pid, Some(1234));
        assert_eq!(e[0].name.as_deref(), Some("sshd"));
        // No `users:` column → visible port, unknown owner.
        assert_eq!(e[1].proto, Proto::Udp);
        assert_eq!(e[1].address(), "127.0.0.53%lo:53");
        assert_eq!(e[1].pid, None);
        assert_eq!(e[1].name, None);
        assert_eq!(e[2].address(), "[::]:80");
        assert_eq!(e[2].pid, Some(800));
        assert_eq!(e[3].name.as_deref(), Some("avahi-daemon"));
    }

    #[test]
    fn ss_process_field() {
        assert_eq!(
            parse_ss_process("users:((\"sshd\",pid=1234,fd=3))"),
            Some(("sshd".to_string(), 1234))
        );
        assert_eq!(parse_ss_process("garbage"), None);
    }

    #[test]
    fn host_port_split() {
        assert_eq!(split_host_port("[::1]:8080"), Some(("[::1]", 8080)));
        assert_eq!(split_host_port("*:5353"), Some(("*", 5353)));
        assert_eq!(split_host_port("*:*"), None);
        assert_eq!(split_host_port("nocolon"), None);
    }
}
