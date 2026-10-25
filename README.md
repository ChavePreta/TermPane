# TermPane

**Organize a flood of terminals without ever leaving your flow.**

[![Website](https://img.shields.io/badge/Website-termpane.com-7aa2f7.svg)](https://termpane.com) [![License: PolyForm Internal Use 1.0.0](https://img.shields.io/badge/License-PolyForm%20Internal%20Use%201.0.0-blue.svg)](LICENSE)

TermPane is a native, lightweight terminal manager for macOS (and Linux next)
that turns a wall of windows into a single, glanceable workspace. Every shell
you open lives as a card in the sidebar — named, renameable, and reorderable —
with the currently running command shown right next to its label.

<img src="https://termpane.com/assets/screenshot-v0.3.1.png" alt="TermPane Screenshot" width="600" style="border: 1px solid black;" />

---

## Features

### Native & lightweight
- **Native Rust + Tauri 2 binary** — small DMG, single-digit-percent CPU at rest. No Chromium bundle, no Electron bloat.
- **WebGL-accelerated rendering** via `xterm.js` for buttery scrolling and redraws on large outputs, with automatic fallback to the DOM renderer if the GPU context drops (e.g. when the Mac wakes from sleep) so a pane never goes blank.
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
- **Attention indicator** — when a program rings the bell (or sends an `OSC 9` / `OSC 777` notification), its card pulses; if it wasn't the card you were viewing, it keeps a dot + highlight until you switch to it. When TermPane isn't focused, it also fires a native OS notification (on by default; toggle in Preferences). See [Get notified when a terminal needs you](#get-notified-when-a-terminal-needs-you).
- **Confirmation dialog** before destroying a terminal, with a heads-up if a process is still running.

### Get notified when a terminal needs you
- **Bell-aware sidebar.** Any program that rings the terminal bell (`\x07`) — a finished `make`, a test run, an alert in a TUI — pulses its sidebar card. If it's a card you weren't looking at, the card keeps a dot + accent highlight until you open it. **Zero config.**
- **OS notifications when you're away.** If TermPane isn't the focused window, the same event raises a native macOS notification (throttled to one per terminal until you visit it). On by default; turn it off in **Preferences → "Notify when a background terminal needs attention."**
- **`OSC 9` / `OSC 777` support.** Tools that request notifications via these standard escape sequences (iTerm2/Ghostty/VS Code style) light up the card too, and their message text rides along to the OS notification.
- **Claude Code.** Install the plugin in **Preferences → Claude Code integration** and Claude's permission requests, questions and finished turns light up the card. (The older bell-script setup in [`integrations/claude-code/`](integrations/claude-code/README.md) still works, but isn't needed anymore.)

### Built for Claude Code
- **Live status on every card** — a spinner while Claude works, the tool it's running, and a clear marker when it needs your permission or is done (with a notification when TermPane is in the background).
- **Sub-tabs** — nest a dev server or shell under the Claude tab that uses it (`⌘⇧T`). Collapsible, drag to nest/un-nest.
- **Files changed per session** — a badge shows what each tab's Claude session wrote; stage them with one click (`⌘⇧S`). Works with several sessions in one folder: files touched by two sessions are flagged, never auto-staged.
- **Follows worktrees** — new tabs and splits open where Claude is actually working.
- **Resume any past session** (`⌘⇧O`) — searchable list of Claude Code sessions, current project first; reopens with `claude --resume`.
- **Claude drives your tabs** — Claude can list tabs, read another tab's output, type commands, and open, focus or close tabs, across every TermPane window.
- **`/fork` into a sub-tab** — the copy opens nested under the current tab as **FORK - <tab>**, already resumed.
- **One plugin, one click** — Preferences → *Claude Code integration* → **Install Claude Code plugin** sets all of this up; outside TermPane the plugin does nothing.
- **Tabs come back after a crash or restart**, with running Claude sessions resumed.
- **Mark as unread** (`⌘⇧U`) and **jump to the next tab that needs you** (`⌘J`); a **Dock badge** counts the tabs waiting for you.
- **Tabs named after their session**, with **branch / worktree** and **open ports** (click to open in the browser) under the name. Renaming a tab names its Claude session too.
- **Watch background work** — a spy button opens a live, read-only view of commands and agents Claude runs in the background; published **artifacts** are listed with links.
- **Copy buttons on Claude's code blocks** — copies the code exactly as written, not as the screen wrapped it; a `{ }` list keeps every block of the session.
- **Diffs per session**, **⌘F** search in the scrollback, **⌘-click file paths** to open them in your editor, and a **list of your prompts** (`⌘⇧L`).
- **⋯ menu on every pane** — all actions for that pane and its tab, each with its shortcut, including **Fork Claude session into a sub-tab**.
- **Jump between your own prompts** in a long Claude session — ▲/▼ on the pane's right edge, or `⌘↑` / `⌘↓` — plus go-to-start / go-to-end buttons (`⌘Home` / `⌘End`).

### Works with Dev Containers
If a project has a [`devcontainer.json`](https://containers.dev) (`.devcontainer/devcontainer.json`, `.devcontainer.json` or `.devcontainer/<name>/devcontainer.json`), TermPane reads it:

- **Port names and protocols** from `portsAttributes` — the card shows "App" instead of `127.0.0.1:80`; ports marked `"onAutoForward": "ignore"` are hidden.
- **Open shell in dev container** in the pane's ⋯ menu — a sub-tab inside the container at `workspaceFolder`, for Docker Compose based configurations (`dockerComposeFile` + `service`). An already-running container is reused, never recreated.
- **Exact URLs** through the spec's tool-specific block, when a port isn't served at `http://127.0.0.1:<port>/`:

  ```jsonc
  "customizations": {
    "termpane": { "portUrls": { "80": "http://app.test/admin" } }
  }
  ```

Other tools ignore the `termpane` block, so it's safe to commit.

### Customize your workspace
- **Rename any card** with a double-click (or F2).
- **Drag-and-drop reorder** — grab a card, drop where you want, blue indicator shows the target slot.
- **Custom terminal shortcuts** — a **+ custom** button beside **+ New terminal** opens a dropdown of named shortcuts you define, each with an optional starting directory, startup command, and shell. Click one to launch a preconfigured terminal — e.g. open Claude in a project folder, or SSH into a host. The card is named after the shortcut. Manage and reorder them (↑/↓) in a built-in editor; they persist across launches.
- **Always-on-top pin** in the sidebar header — keep TermPane floating over your editor while you debug. Persists across launches.
- **Preferences dialog** for font family, font size (8–32 px), line height, cursor blink, bell notifications, reopening tabs after a crash, and the Claude Code integration. Live-updates every open terminal.

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

| Action                                    | macOS               | Linux                  |
| ----------------------------------------- | ------------------- | ---------------------- |
| New window (separate instance)            | `⌘N`                | `Ctrl+Shift+N`         |
| New terminal                              | `⌘T`                | `Ctrl+Shift+T`         |
| New sub-tab of the active tab             | `⌘⇧T`               | `Ctrl+Shift+Alt+T`     |
| Jump to next tab needing you              | `⌘J`                | `Ctrl+Shift+J`         |
| Mark active tab unread / read             | `⌘⇧U`               | `Ctrl+Shift+Alt+U`     |
| Stage files Claude changed in active tab  | `⌘⇧S`               | `Ctrl+Shift+Alt+S`     |
| Resume a Claude session                   | `⌘⇧O`               | `Ctrl+Shift+Alt+O`     |
| Redraw terminals (keeps scrollback)       | `⌘⇧R`               | `Ctrl+Shift+Alt+R`     |
| Previous / next prompt typed to Claude    | `⌘↑` / `⌘↓`         | `Ctrl+Shift+↑` / `Ctrl+Shift+↓` |
| Start / end of scrollback                 | `⌘Home` / `⌘End`    | `Ctrl+Shift+Home` / `Ctrl+Shift+End` |
| Show all keyboard shortcuts               | `⌘?`                | `Ctrl+Shift+Alt+/`     |
| Find in scrollback / next / previous      | `⌘F` / `⌘G` / `⌘⇧G` | `Ctrl+Shift+F` / `Ctrl+Shift+G` / `Ctrl+Shift+Alt+G` |
| List your prompts to Claude               | `⌘⇧L`               | `Ctrl+Shift+Alt+L`     |
| Open a file path in your editor           | `⌘`-click           | `Ctrl`-click           |
| Copy exactly as displayed (⌘C joins wrapped lines) | `⌘⇧C`      | `Ctrl+Shift+Alt+C`     |
| Close pane / terminal (with confirmation) | `⌘W`                | `Ctrl+Shift+W`         |
| Split horizontal / vertical               | `⌘D` / `⌘⇧D`        | `Ctrl+Shift+D` / `Ctrl+Shift+Alt+D` |
| Flip orientation of parent split          | `⌘/`                | `Ctrl+Shift+/`         |
| Extract active pane to new card           | `⌘⇧E`               | `Ctrl+Shift+Alt+E`     |
| Clear viewport + scrollback               | `⌘K`                | `Ctrl+Shift+K`         |
| Next / previous terminal card             | `⌘⇧→` / `⌘⇧←`       | `Ctrl+Shift+Alt+→` / `Ctrl+Shift+Alt+←` |
| Toggle sidebar                            | `⌘B`                | `Ctrl+Shift+B`         |
| Font zoom in / out / reset                | `⌘=` / `⌘-` / `⌘0`  | `Ctrl+Shift+=` / `Ctrl+Shift+-` / `Ctrl+Shift+0` |
| Rename active card                        | Double-click / `F2` | Double-click / `F2`    |
| Cancel rename / close dialog              | `Esc`               | `Esc`                  |

Any `Ctrl+letter` (Ctrl+R reverse search, Ctrl+A line start, Ctrl+E line end, Ctrl+C interrupt, …) is routed straight to the active PTY.

### Drag-and-drop

Drag a card from the sidebar **into the terminal area** to merge it as a split of the terminal you drop on. Hold `Shift` while releasing to make the new split vertical instead of horizontal. Dragging a card back into the sidebar still reorders the list as before.

### Broadcast input

When the active terminal has 2 or more panes, a **Broadcast** toggle appears in the status bar. Turn it on to mirror every keystroke from any pane to every other pane in the same terminal — handy for running the same command across multiple SSH sessions. The terminal gets a red outline while broadcast is active. Output stays independent per pane.

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
