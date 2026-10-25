//! Dev Containers support (https://containers.dev): a tab whose project has a
//! `devcontainer.json` gets its port names and protocols on the card
//! (`portsAttributes`) and a "shell in the dev container" action.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortAttributes {
    pub label: Option<String>,
    pub protocol: Option<String>,
    /// `onAutoForward: "ignore"` — the project says this port is not for people.
    pub ignore: bool,
    /// The address to open, from `customizations.termpane.portUrls` (the
    /// spec's place for tool-specific settings) — for when it isn't
    /// `<protocol>://127.0.0.1:<port>/`, e.g. another host or a path.
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DevContainer {
    pub name: String,
    /// The folder the configuration belongs to (holds `.devcontainer/`).
    pub root: PathBuf,
    pub compose_files: Vec<PathBuf>,
    pub service: Option<String>,
    pub workspace_folder: Option<String>,
    pub ports: HashMap<u16, PortAttributes>,
}

/// The configuration of the project `cwd` is in: the nearest
/// `.devcontainer/devcontainer.json`, `.devcontainer.json` or
/// `.devcontainer/<name>/devcontainer.json` walking up from `cwd`, never above
/// `stop` (the repository root, so a worktree doesn't pick up the main
/// checkout's configuration).
pub fn find(cwd: &Path, stop: Option<&Path>) -> Option<DevContainer> {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        for candidate in candidates(d) {
            if candidate.is_file() {
                return load(&candidate, d);
            }
        }
        if stop.is_some_and(|s| s == d) || Some(d) == dirs::home_dir().as_deref() {
            break;
        }
        dir = d.parent();
    }
    None
}

fn candidates(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![
        dir.join(".devcontainer").join("devcontainer.json"),
        dir.join(".devcontainer.json"),
    ];
    if let Ok(entries) = std::fs::read_dir(dir.join(".devcontainer")) {
        let mut nested: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("devcontainer.json"))
            .collect();
        nested.sort();
        out.extend(nested);
    }
    out
}

fn load(path: &Path, root: &Path) -> Option<DevContainer> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&strip_jsonc(&text)).ok()?;
    let config_dir = path.parent()?;
    let compose_files = match &v["dockerComposeFile"] {
        Value::String(s) => vec![config_dir.join(s)],
        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).map(|s| config_dir.join(s)).collect(),
        _ => Vec::new(),
    };
    let mut ports: HashMap<u16, PortAttributes> = HashMap::new();
    for (key, attrs) in v["portsAttributes"].as_object().into_iter().flatten() {
        // Keys are ports ("3000"), and the spec also allows ranges/regexes,
        // which a card showing concrete ports can't use.
        let Ok(port) = key.parse::<u16>() else { continue };
        ports.insert(
            port,
            PortAttributes {
                label: attrs["label"].as_str().map(str::to_string),
                protocol: attrs["protocol"].as_str().map(str::to_ascii_lowercase),
                ignore: attrs["onAutoForward"] == "ignore",
                url: None,
            },
        );
    }
    for (key, url) in v["customizations"]["termpane"]["portUrls"].as_object().into_iter().flatten() {
        let (Ok(port), Some(url)) = (key.parse::<u16>(), url.as_str()) else { continue };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        ports
            .entry(port)
            .or_insert(PortAttributes { label: None, protocol: None, ignore: false, url: None })
            .url = Some(url.to_string());
    }
    Some(DevContainer {
        name: v["name"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| root.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()),
        root: root.to_path_buf(),
        compose_files,
        service: v["service"].as_str().map(str::to_string),
        workspace_folder: v["workspaceFolder"].as_str().map(str::to_string),
        ports,
    })
}

/// devcontainer.json is JSON with comments (`//`, `/* */`) and trailing
/// commas; turn it into plain JSON. Strings are left untouched.
pub fn strip_jsonc(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match (c, chars.get(i + 1)) {
            ('"', _) => {
                in_string = true;
                out.push(c);
                i += 1;
            }
            ('/', Some('/')) => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            (',', _) => {
                // Drop a comma whose next meaningful character closes a container.
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if !matches!(chars.get(j), Some('}') | Some(']')) {
                    out.push(c);
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The command a tab runs to get a shell inside the dev container: start the
/// service only if it isn't running (never recreate one already up — other
/// tools or scripts may own it), then exec into it at the workspace folder.
/// Plain `docker compose` works for containers started outside the Dev
/// Containers tooling too (as with `docker compose up`).
pub fn shell_command(dc: &DevContainer) -> Option<String> {
    let service = dc.service.as_ref()?;
    if dc.compose_files.is_empty() {
        return None;
    }
    let files: Vec<String> = dc
        .compose_files
        .iter()
        .map(|f| format!("-f {}", quote(&f.to_string_lossy())))
        .collect();
    let compose = format!("docker compose {}", files.join(" "));
    let workdir = dc
        .workspace_folder
        .as_ref()
        .map(|w| format!("-w {} ", quote(w)))
        .unwrap_or_default();
    Some(format!(
        "[ -n \"$({compose} ps -q --status running {svc})\" ] || {compose} up -d {svc}; {compose} exec {workdir}{svc} sh -c 'command -v bash >/dev/null && exec bash || exec sh'",
        svc = quote(service),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_comments_and_trailing_commas_but_not_strings() {
        let src = r#"// top
{
  "name": "x // not a comment", /* block */
  "url": "http://a/*b*/",
  "list": [1, 2,],
  "obj": { "k": "v", },
}"#;
        let v: Value = serde_json::from_str(&strip_jsonc(src)).unwrap();
        assert_eq!(v["name"], "x // not a comment");
        assert_eq!(v["url"], "http://a/*b*/");
        assert_eq!(v["list"], serde_json::json!([1, 2]));
        assert_eq!(v["obj"]["k"], "v");
    }

    #[test]
    fn finds_and_reads_a_compose_based_configuration() {
        let tmp = std::env::temp_dir().join(format!("termpane-dc-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".devcontainer")).unwrap();
        std::fs::create_dir_all(tmp.join("app/src")).unwrap();
        std::fs::write(
            tmp.join(".devcontainer/devcontainer.json"),
            r#"{ // app
  "name": "demo",
  "dockerComposeFile": ["../docker-compose.yml"],
  "service": "web",
  "workspaceFolder": "/var/www",
  "portsAttributes": {
    "80": { "label": "App", "protocol": "http" },
    "6379": { "label": "Redis", "onAutoForward": "ignore" },
    "3000-3010": { "label": "range" },
  },
  "customizations": { "termpane": { "portUrls": { "80": "http://app.test/admin", "81": "ftp://x" } } },
}"#,
        )
        .unwrap();
        let dc = find(&tmp.join("app/src"), Some(&tmp)).expect("found");
        assert_eq!(dc.name, "demo");
        assert_eq!(dc.service.as_deref(), Some("web"));
        assert_eq!(dc.compose_files, vec![tmp.join(".devcontainer/../docker-compose.yml")]);
        assert_eq!(dc.ports[&80].label.as_deref(), Some("App"));
        assert!(dc.ports[&6379].ignore);
        assert_eq!(dc.ports[&80].url.as_deref(), Some("http://app.test/admin"));
        assert_eq!(dc.ports.len(), 2, "ranges and non-http urls are skipped");
        let cmd = shell_command(&dc).unwrap();
        assert!(cmd.contains("ps -q --status running 'web')\" ] || "));
        assert!(cmd.contains("up -d 'web';"));
        assert!(cmd.contains("exec -w '/var/www' 'web'"));
        // Not above the stop directory.
        assert!(find(&tmp.join("app"), Some(&tmp.join("app"))).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
