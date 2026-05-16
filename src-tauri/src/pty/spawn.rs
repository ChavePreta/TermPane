use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct SpawnResult {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub reader: Box<dyn Read + Send>,
    pub child: Box<dyn Child + Send + Sync>,
    pub pid: Option<u32>,
}

/// Spawns the user's default shell ($SHELL, falling back to /bin/zsh) attached to a PTY.
pub fn spawn_shell(cwd: Option<&str>) -> anyhow::Result<SpawnResult> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| anyhow::anyhow!("openpty failed: {e}"))?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");

    let cwd_path = cwd
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from));
    if let Some(p) = cwd_path {
        cmd.cwd(p);
    }

    for (k, v) in std::env::vars_os() {
        cmd.env(k, v);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "TermPane");
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));

    // GUI apps launched by launchd (macOS) or a non-login session (Linux) often
    // do not inherit a UTF-8 locale. Without it, zsh in emacs keymap treats
    // bytes with the high bit set as meta sequences (Alt+x) and strips that
    // bit on input — turning a typed `á` (UTF-8 0xC3 0xA1) into `C!` (0x43 0x21).
    // Force a sane UTF-8 locale unless the user already has one.
    ensure_utf8_locale(&mut cmd);

    // Shell integration: for zsh we redirect ZDOTDIR to a shim that sources
    // the user's `.zshrc` and forces `bindkey -e` (emacs keymap), so shortcuts
    // like Ctrl+R = reverse search work even when an inherited $EDITOR like
    // `vim`/`vi` would otherwise make zsh default to viins.
    if is_zsh(&shell) {
        match ensure_zsh_shim() {
            Ok(dir) => {
                cmd.env("ZDOTDIR", &dir);
            }
            Err(e) => tracing::warn!("failed to prepare zsh shim: {e}"),
        }
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| anyhow::anyhow!("shell spawn failed: {e}"))?;
    let pid = child.process_id();

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| anyhow::anyhow!("take_writer failed: {e}"))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| anyhow::anyhow!("try_clone_reader failed: {e}"))?;

    Ok(SpawnResult {
        master: pair.master,
        writer,
        reader,
        child,
        pid,
    })
}

fn looks_utf8(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    s.contains("utf-8") || s.contains("utf8")
}

fn ensure_utf8_locale(cmd: &mut CommandBuilder) {
    let lc_all = std::env::var("LC_ALL").ok();
    let lc_ctype = std::env::var("LC_CTYPE").ok();
    let lang = std::env::var("LANG").ok();

    if lc_all.as_deref().is_some_and(looks_utf8)
        || lc_ctype.as_deref().is_some_and(looks_utf8)
        || lang.as_deref().is_some_and(looks_utf8)
    {
        return;
    }

    let candidate = detect_preferred_utf8_locale().unwrap_or_else(|| "en_US.UTF-8".to_string());
    cmd.env("LANG", &candidate);
    cmd.env("LC_CTYPE", &candidate);
}

#[cfg(target_os = "macos")]
fn detect_preferred_utf8_locale() -> Option<String> {
    let out = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // AppleLocale may include @ modifiers (e.g. "pt_BR@calendar=gregorian"); strip them.
    let base = trimmed.split('@').next().unwrap_or(trimmed);
    Some(format!("{base}.UTF-8"))
}

#[cfg(not(target_os = "macos"))]
fn detect_preferred_utf8_locale() -> Option<String> {
    // C.UTF-8 is the most portable fallback on modern Linux distros.
    Some("C.UTF-8".to_string())
}

fn is_zsh(shell: &str) -> bool {
    Path::new(shell)
        .file_name()
        .map(|f| f == "zsh")
        .unwrap_or(false)
}

/// Ensures the zsh shim directory exists and contains startup files that
/// delegate to the ones in `$HOME` and append `bindkey -e` at the end.
fn ensure_zsh_shim() -> anyhow::Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("data_dir unavailable"))?;
    let shim = base.join("TermPane").join("shell-integration").join("zsh");
    std::fs::create_dir_all(&shim)?;

    let banner =
        "# TermPane shell integration — auto-generated, do not edit manually.\n";

    let zshenv = format!(
        "{banner}[[ -f \"$HOME/.zshenv\" ]] && source \"$HOME/.zshenv\"\n",
    );
    let zprofile = format!(
        "{banner}[[ -f \"$HOME/.zprofile\" ]] && source \"$HOME/.zprofile\"\n",
    );
    let zshrc = format!(
        "{banner}\
[[ -f \"$HOME/.zshrc\" ]] && source \"$HOME/.zshrc\"\n\
# Force emacs keymap so Ctrl+R, Ctrl+A, Ctrl+E, etc. behave like in a native\n\
# terminal regardless of any inherited $EDITOR/$VISUAL.\n\
bindkey -e\n",
    );
    let zlogin = format!(
        "{banner}[[ -f \"$HOME/.zlogin\" ]] && source \"$HOME/.zlogin\"\n",
    );

    write_if_changed(&shim.join(".zshenv"), &zshenv)?;
    write_if_changed(&shim.join(".zprofile"), &zprofile)?;
    write_if_changed(&shim.join(".zshrc"), &zshrc)?;
    write_if_changed(&shim.join(".zlogin"), &zlogin)?;

    Ok(shim)
}

fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == content {
            return Ok(());
        }
    }
    std::fs::write(path, content)
}
