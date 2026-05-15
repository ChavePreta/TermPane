# TermPane

**Organize a flood of terminals without ever leaving your flow.**

[![Website](https://img.shields.io/badge/Website-termpane.com-7aa2f7.svg)](https://termpane.com) [![License: PolyForm Internal Use 1.0.0](https://img.shields.io/badge/License-PolyForm%20Internal%20Use%201.0.0-blue.svg)](LICENSE)

TermPane is a native, lightweight terminal manager for macOS (and Linux next)
that turns a wall of windows into a single, glanceable workspace. Every shell
you open lives as a card in the sidebar — named, renameable, and reorderable —
with the currently running command shown right next to its label.

---

## Features

### Native & lightweight
- **Native Rust + Tauri 2 binary** — small DMG, single-digit-percent CPU at rest. No Chromium bundle, no Electron bloat.
- **WebGL-accelerated rendering** via `xterm.js` for buttery scrolling and redraws on large outputs.
- **Real PTYs** powered by `portable-pty` (the same library WezTerm uses). No emulation tricks — your shell is your shell.

### Full shell compatibility
- **Zsh, bash, fish, nu, anything in `$SHELL`** — TermPane spawns your default login shell as your OS does.
- **Every TUI works:** vim, htop, less, top, k9s, btop, lazygit, tmux.
- **SSH sessions** behave identically to a native terminal.
- **256-color + true color** (`COLORTERM=truecolor`).
- **All signal/control bytes routed correctly:** Ctrl+C, Ctrl+Z, Ctrl+D, Ctrl+R (reverse search), Ctrl+A/E (line nav), Ctrl+L, and the rest.

### Sidebar that knows what's running
- **Card-based terminal list** on the left — every shell at a glance.
- **Live foreground-command tracking** — the running process appears in italic, muted color, next to the terminal label (`Term#1 — npm`).
- **Multi-pane aggregation:** all running commands joined by `/`, truncated with `…` when too long.
- **Hover reveals** open timestamp and a close button.
- **Confirmation dialog** before destroying a terminal, with a heads-up if a process is still running.

### Customize your workspace
- **Rename any card** with a double-click (or F2).
- **Drag-and-drop reorder** — grab a card, drop where you want, blue indicator shows the target slot.
- **Always-on-top pin** in the sidebar header — keep TermPane floating over your editor while you debug. Persists across launches.
- **Preferences dialog** for font family, font size (8–32 px), line height, and cursor blink. Live-updates every open terminal.

### Splits, the way you want them
- **Horizontal split** (`⌘D`) — panes side by side.
- **Vertical split** (`⌘⇧D`) — panes stacked.
- **Recursive splits** to any depth, with draggable dividers to resize.
- **Close a single pane** (`⌘W`) without closing the whole terminal.

### Themes that follow you
- **Light**, **Dark** (Tokyo Night Storm), or **System** (auto-follows macOS dark/light setting).
- **One-click switcher** in the sidebar header.
- **Live theme switch** — xterm and UI update instantly, no relaunch.

### Privacy-respecting shell integration
- **TermPane never edits your `.zshrc`.**
- **Transparent ZDOTDIR shim** — your `.zshenv`, `.zprofile`, `.zshrc`, and `.zlogin` are sourced from `$HOME` as usual.
- Identifies itself politely: `TERM_PROGRAM=TermPane`, `TERM_PROGRAM_VERSION=<x.y.z>`.
- **No telemetry, no analytics, no network calls.**

---

## Install

### macOS

1. Download `TermPane_x.y.z_aarch64.dmg` (Apple Silicon) or the Intel build from the [Releases](https://github.com/ChavePreta/TermPane/releases) page.
2. Open the DMG and drag **TermPane.app** to **Applications**.
3. Because the build is not yet signed with an Apple Developer ID, macOS Gatekeeper will block it the first time. Run **once**:

   ```sh
   xattr -dr com.apple.quarantine /Applications/TermPane.app
   ```

   Or, alternatively, right-click the app → **Open** → confirm.

### Linux

- **Debian / Ubuntu (.deb):**

  ```sh
  sudo dpkg -i TermPane_x.y.z_amd64.deb
  ```

- **AppImage:**

  ```sh
  chmod +x TermPane_x.y.z_amd64.AppImage
  ./TermPane_x.y.z_amd64.AppImage
  ```

---

## Keyboard shortcuts

| Shortcut             | Action                                    |
| -------------------- | ----------------------------------------- |
| `⌘T`                 | New terminal                              |
| `⌘W`                 | Close pane / terminal (with confirmation) |
| `⌘D` / `⌘⇧D`         | Split horizontal / vertical               |
| `⌘K`                 | Clear viewport + scrollback               |
| `⌘=` / `⌘-` / `⌘0`   | Font zoom in / out / reset                |
| Double-click / `F2`  | Rename active card                        |
| `Esc`                | Cancel rename / close dialog              |

Any `Ctrl+letter` (Ctrl+R reverse search, Ctrl+A line start, Ctrl+E line end, Ctrl+C interrupt, …) is routed straight to the active PTY.

---

## License

**[PolyForm Internal Use License 1.0.0](LICENSE).**

You may read, modify, and use TermPane for the internal business operations of
you or your company — including unlimited personal use. You may **not**
redistribute the software, sublicense it, or bundle it inside a product or
service offered to third parties. See the full text in [LICENSE](LICENSE).

---

## Support & Links

🌐 **Website:** <https://termpane.com>
☕ **Buy me a coffee:** <https://buymeacoffee.com/chavepreta>
🐛 **Issues & ideas:** <https://github.com/ChavePreta/TermPane/issues>
