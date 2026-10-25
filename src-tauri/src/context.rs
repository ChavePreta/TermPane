//! Slow-changing context shown on tab cards: the git branch / worktree of a
//! pane's working directory and the TCP ports its processes listen on.
//! Refreshed by the monitor every few seconds, not every tick.

use crate::model::GitInfo;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Git context of `cwd`, or None outside a repository. One `git rev-parse`.
pub fn git_info(cwd: &str) -> Option<GitInfo> {
    let out = std::process::Command::new("git")
        .args(["-C", cwd, "rev-parse", "--path-format=absolute"])
        .args(["--abbrev-ref", "HEAD", "--show-toplevel", "--git-dir", "--git-common-dir"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let branch = lines.next()?.trim().to_string();
    let toplevel = lines.next()?.trim().to_string();
    let git_dir = lines.next()?.trim().to_string();
    let common_dir = lines.next()?.trim().to_string();
    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    // A linked worktree has its own git dir under the main repo's common dir.
    let worktree = (git_dir != common_dir).then(|| name(&toplevel));
    let repo = Path::new(&common_dir)
        .parent()
        .map(|p| name(&p.to_string_lossy()))
        .unwrap_or_else(|| name(&toplevel));
    Some(GitInfo {
        repo,
        branch: if branch == "HEAD" { "detached".to_string() } else { branch },
        worktree,
        toplevel,
    })
}

/// Caches `git_info` per directory so a refresh costs one git call per
/// directory every `TTL`, however many panes share it.
#[derive(Default)]
pub struct GitCache {
    entries: HashMap<String, (Instant, Option<GitInfo>)>,
}

const TTL: Duration = Duration::from_secs(10);

impl GitCache {
    pub fn get(&mut self, cwd: &str) -> Option<GitInfo> {
        if let Some((at, info)) = self.entries.get(cwd) {
            if at.elapsed() < TTL {
                return info.clone();
            }
        }
        let info = git_info(cwd);
        self.entries.insert(cwd.to_string(), (Instant::now(), info.clone()));
        info
    }
}

/// Caches the dev container configuration per directory (re-read every
/// `TTL`, so edits to devcontainer.json show up within seconds).
#[derive(Default)]
pub struct DevContainerCache {
    entries: HashMap<String, (Instant, Option<crate::devcontainer::DevContainer>)>,
}

impl DevContainerCache {
    /// The configuration for `cwd`, searched no higher than `toplevel`.
    pub fn get(&mut self, cwd: &str, toplevel: Option<&str>) -> Option<crate::devcontainer::DevContainer> {
        if let Some((at, dc)) = self.entries.get(cwd) {
            if at.elapsed() < TTL {
                return dc.clone();
            }
        }
        let dc = crate::devcontainer::find(Path::new(cwd), toplevel.map(Path::new));
        self.entries.insert(cwd.to_string(), (Instant::now(), dc.clone()));
        dc
    }
}

/// Listening TCP addresses by owning PID, from one `lsof` call for the whole
/// machine. Wildcard binds (`*`, `0.0.0.0`, `[::]`) and IPv6 loopback read as
/// 127.0.0.1, the address to open in a browser.
pub fn listening_ports() -> HashMap<u32, Vec<String>> {
    let mut by_pid: HashMap<u32, Vec<String>> = HashMap::new();
    let Ok(out) = std::process::Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"])
        .output()
    else {
        return by_pid;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut pid: Option<u32> = None;
    for line in text.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(addr), Some(p)) = (line.strip_prefix('n'), pid) {
            let Some((host, port)) = addr.rsplit_once(':') else { continue };
            let host = match host {
                "*" | "0.0.0.0" | "[::]" | "[::1]" => "127.0.0.1",
                h => h,
            };
            let entry = format!("{host}:{port}");
            let list = by_pid.entry(p).or_default();
            if !list.contains(&entry) {
                list.push(entry);
            }
        }
    }
    by_pid
}

/// Database, cache and broker ports: shown, but not offered as a web page.
const NON_HTTP_PORTS: &[u16] = &[
    1433, 2181, 3306, 3307, 5432, 5433, 5672, 6379, 6380, 9042, 9092, 11211, 15672, 27017,
];

/// Card entries for a pane's listening addresses: named and linked from the
/// project's devcontainer.json when it has one (ports it marks `ignore` are
/// dropped), else linked as `http://127.0.0.1:<port>/` when local and not a
/// known database/cache port.
pub fn describe_ports(
    addresses: &[String],
    dev: Option<&crate::devcontainer::DevContainer>,
) -> Vec<crate::model::PortInfo> {
    let mut out = Vec::new();
    for address in addresses {
        let port = port_of(address);
        let attrs = port.and_then(|p| dev.and_then(|d| d.ports.get(&p)));
        if attrs.is_some_and(|a| a.ignore) {
            continue;
        }
        let local = address.starts_with("127.0.0.1:") || address.starts_with("localhost:");
        let url = attrs.and_then(|a| a.url.clone()).or_else(|| {
            let protocol = attrs.and_then(|a| a.protocol.clone());
            let web = protocol.is_some() || !port.is_some_and(|p| NON_HTTP_PORTS.contains(&p));
            (local && web).then(|| {
                format!("{}://{}/", protocol.as_deref().unwrap_or("http"), address)
            })
        });
        out.push(crate::model::PortInfo {
            address: address.clone(),
            label: attrs.and_then(|a| a.label.clone()),
            url,
        });
    }
    out
}

/// Ports the OS hands out on its own (macOS/BSD ephemeral range): helper and
/// internal sockets, never a server someone asked for by number.
pub const EPHEMERAL_START: u16 = 49152;

pub fn port_of(addr: &str) -> Option<u16> {
    addr.rsplit_once(':').and_then(|(_, p)| p.parse().ok())
}

/// Host ports published by Docker Compose containers, keyed by the compose
/// project's working directory (the `com.docker.compose.project.working_dir`
/// label) — so a tab in a repo or a worktree can show its own containers.
#[derive(Default, Clone)]
pub struct DockerPorts {
    pub by_dir: Vec<(String, Vec<String>)>,
}

impl DockerPorts {
    /// Ports of the most specific compose project containing `cwd`: a tab at
    /// the repo root doesn't get the containers of worktrees nested under it.
    pub fn for_dir(&self, cwd: &str) -> Vec<String> {
        let cwd = cwd.trim_end_matches('/');
        self.by_dir
            .iter()
            .filter(|(dir, _)| cwd == dir || cwd.starts_with(&format!("{dir}/")))
            .max_by_key(|(dir, _)| dir.len())
            .map(|(_, ports)| ports.clone())
            .unwrap_or_default()
    }
}

// GUI apps don't inherit the shell's PATH; look where Docker Desktop and
// package managers put the CLI.
fn docker_cli() -> Option<std::path::PathBuf> {
    let mut candidates = vec![
        std::path::PathBuf::from("/usr/local/bin/docker"),
        std::path::PathBuf::from("/opt/homebrew/bin/docker"),
        std::path::PathBuf::from("/usr/bin/docker"),
        std::path::PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker"),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".docker/bin/docker"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn read_docker_ports() -> DockerPorts {
    let Some(cli) = docker_cli() else { return DockerPorts::default() };
    let Ok(out) = std::process::Command::new(cli)
        .args([
            "ps",
            "--format",
            "{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Ports}}",
        ])
        .output()
    else {
        return DockerPorts::default();
    };
    let mut by_dir: Vec<(String, Vec<String>)> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((dir, ports)) = line.split_once('\t') else { continue };
        let dir = dir.trim().trim_end_matches('/');
        if dir.is_empty() {
            continue;
        }
        // "0.0.0.0:80->80/tcp, [::]:80->80/tcp"
        let mut published: Vec<String> = Vec::new();
        for item in ports.split(", ") {
            let Some((host, _container)) = item.split_once("->") else { continue };
            let Some(port) = port_of(host) else { continue };
            let addr = format!("127.0.0.1:{port}");
            if !published.contains(&addr) {
                published.push(addr);
            }
        }
        match by_dir.iter_mut().find(|(d, _)| d == dir) {
            Some((_, list)) => list.extend(published.into_iter().filter(|p| !list.contains(p)).collect::<Vec<_>>()),
            None => by_dir.push((dir.to_string(), published)),
        }
    }
    DockerPorts { by_dir }
}

/// Keeps `DockerPorts` fresh on a thread of its own: `docker ps` can stall
/// when Docker Desktop is starting or wedged, and that must never hold up the
/// monitor. Readers get the last good snapshot.
pub fn start_docker_watch() -> std::sync::Arc<parking_lot::Mutex<DockerPorts>> {
    let shared = std::sync::Arc::new(parking_lot::Mutex::new(DockerPorts::default()));
    if docker_cli().is_none() {
        return shared;
    }
    let writer = shared.clone();
    std::thread::Builder::new()
        .name("docker-ports".into())
        .spawn(move || loop {
            let ports = read_docker_ports();
            *writer.lock() = ports;
            std::thread::sleep(Duration::from_secs(10));
        })
        .ok();
    shared
}

#[cfg(test)]
mod tests {
    use super::DockerPorts;

    #[test]
    fn picks_the_most_specific_compose_project() {
        let d = DockerPorts {
            by_dir: vec![
                ("/r/app".into(), vec!["127.0.0.1:80".into()]),
                ("/r/app/.claude/worktrees/feat".into(), vec!["127.0.0.1:8085".into()]),
            ],
        };
        assert_eq!(d.for_dir("/r/app"), vec!["127.0.0.1:80"]);
        assert_eq!(d.for_dir("/r/app/src"), vec!["127.0.0.1:80"]);
        assert_eq!(d.for_dir("/r/app/.claude/worktrees/feat"), vec!["127.0.0.1:8085"]);
        assert!(d.for_dir("/r/other").is_empty());
        assert!(d.for_dir("/r/application").is_empty());
    }
}
