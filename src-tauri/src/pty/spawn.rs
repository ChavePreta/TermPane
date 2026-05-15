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
