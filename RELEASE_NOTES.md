# Release Notes — v0.3.0 (2026-05-23)

A feature release built around composition: you can now rearrange terminals into splits with a drag instead of starting over, flip a split's orientation without breaking and re-creating it, drive several panes at once, and use the app comfortably on Linux.

## ✨ New: Drag a sidebar card into a terminal to merge them as a split

Until now, the only way to turn two separate cards into a split was: close one, split the other, paste your work back, recover the cwd, etc. Now you just drag.

- Pick up any card from the sidebar.
- Drop it on the **terminal area** (anywhere over the panes on the right) instead of back on the sidebar.
- The dragged card disappears and its terminal's whole layout — single pane or even nested splits — becomes a sibling of the pane you dropped on, inside a new split.
- Hold `Shift` while releasing to make the new split vertical; release without `Shift` for horizontal.

Every PTY keeps running across the merge — no shell respawns, no scrollback loss, no foreground process interruption. Dragging back to a slot in the sidebar still reorders cards exactly like before.

Under the hood, the target pane host got a `data-pane-id` attribute and the sidebar's existing pointer-drag now uses `document.elementFromPoint` to resolve which pane the cursor is currently over; the new `AppState::merge_terminal_into_pane` performs the layout-tree splice and moves the `PaneData` map across in one write lock, then the source terminal is removed.

## ✨ New: Flip a split's orientation in place — `⌘/` / `Ctrl+Shift+/`

Started horizontal but wished it were vertical? Press `⌘/` (macOS) or `Ctrl+Shift+/` (Linux). The split that immediately contains the focused pane flips between horizontal and vertical. No re-creation, no PTYs reborn, no scrollback lost — just the layout direction toggles. In nested splits, only the immediate parent flips.

Backend-side this is `LayoutNode::flip_parent_of(pane_id)`: a depth-first walk that hits the deepest match first, ensuring the nearest enclosing split is the one that turns.

## ✨ New: Broadcast typing across every pane in a split

When the active terminal has 2 or more panes, a **Broadcast** toggle now appears in the status bar. Turn it on and every keystroke you type into any pane is mirrored to every other pane in the same terminal — handy for running the same command on several SSH sessions, scripting bulk maintenance, or just synchronizing setup steps across worktrees.

- Toggle from the status-bar button (only visible when broadcast is meaningful, i.e. ≥ 2 panes).
- The terminal gets a red outline while broadcast is active, so you can't forget it's on.
- Output is **not** mirrored — each pane shows its own results. Only input (your keystrokes) goes to all panes.
- The mode disables itself automatically if the pane count drops below 2 (e.g. you close panes).
- State is in-memory only; broadcast resets on app restart, by design.

Implementation: a new `write_input_broadcast(terminal_id, data)` IPC command iterates `layout.leaves()` for that terminal and writes the bytes to every PTY in a single round-trip; the frontend's `term.onData` handler in `TerminalView` checks a `$broadcastEnabled: Set<TerminalId>` store before deciding whether to send to one pane or all.

## ✨ New: Full Linux keyboard shortcut layer

Every macOS `⌘`-based app shortcut now has a Linux equivalent following the GNOME Terminal / Konsole convention. `Ctrl+Shift+letter` is the app-mod on Linux, leaving plain `Ctrl+letter` available as a PTY control byte (Ctrl+R reverse-search, Ctrl+A line start, Ctrl+C interrupt, etc. all still flow through). When a macOS shortcut adds Shift as a sub-modifier (e.g. `⌘⇧D` for vertical split), the Linux mapping uses Alt for that role (`Ctrl+Shift+Alt+D`) — since Shift is already part of the base.

| Action                          | macOS               | Linux               |
| ------------------------------- | ------------------- | ------------------- |
| New terminal                    | `⌘T`                | `Ctrl+Shift+T`      |
| Close pane / terminal           | `⌘W`                | `Ctrl+Shift+W`      |
| Split horizontal                | `⌘D`                | `Ctrl+Shift+D`      |
| Split vertical                  | `⌘⇧D`               | `Ctrl+Shift+Alt+D`  |
| **Flip parent split**           | `⌘/`                | `Ctrl+Shift+/`      |
| Extract pane to a new card      | `⌘⇧E`               | `Ctrl+Shift+Alt+E`  |
| Toggle sidebar                  | `⌘B`                | `Ctrl+Shift+B`      |
| Clear viewport + scrollback     | `⌘K`                | `Ctrl+Shift+K`      |
| Font zoom in / out / reset      | `⌘=` / `⌘-` / `⌘0`  | `Ctrl+Shift+=` / `Ctrl+Shift+-` / `Ctrl+Shift+0` |

The status bar and "Hide/Show sidebar" tooltips automatically display the right labels for the current OS. Platform is detected at startup via a new `get_platform` Tauri command (`std::env::consts::OS`) and exposed as `$platform` in `src/lib/platform.ts`.

## 🔄 Reverted: WebGL glyph atlas mitigations from v0.2.2

The v0.2.2 release added three measures to fight glyph corruption in long-running Claude Code / `htop` sessions: waiting for `document.fonts.ready` before opening the terminal, an `onContextLoss` listener that fell back to the canvas renderer, and `clearTextureAtlas()` calls on font/preference changes and on viewport resizes. In practice, these made rendering **worse**, not better, so they've been removed.

The v0.2.2 **mouse-wheel handler** is independent and is preserved — explicit `scrollLines()` driving with mouse-tracking passthrough still keeps `vim`, `less`, `htop` and friends happy.

---

# Release Notes — v0.2.2 (2026-05-20)

A fix-and-CI release: a long-running rendering bug squashed, the mouse-wheel scroll-drift fully resolved, and Linux artifacts now ship from CI.

## 🐛 Fixed: WebGL glyph atlas corruption during long sessions

Under heavy redraw load — most visibly during long Claude Code sessions and busy `htop`/`btop` views — the WebGL renderer would gradually desynchronize its glyph atlas, replacing on-screen text with garbled Unicode-like noise until a window resize forced an atlas rebuild. TermPane now:

- Waits for `document.fonts.ready` before opening the terminal, so the initial atlas is built with the correct font metrics.
- Listens for the WebGL renderer's `onContextLoss` event and falls back to the canvas renderer when the context is dropped (Chromium/WebKit shed WebGL contexts under memory pressure).
- Calls `clearTextureAtlas()` on every font/preference change and whenever the viewport's `cols`/`rows` change.

## 🐛 Fixed: Mouse-wheel scroll-down getting stuck under heavy output

The 0.2.1 mitigation worked for steady-state cases but broke down when output was streaming in fast. The previous handler still depended on xterm's internal wheel routing, which fell out of sync under aggressive redraws. TermPane now installs its own wheel listener on the host container and drives `term.scrollLines()` explicitly, deriving a sane line count from `fontSize × lineHeight`. The listener bails out (forwarding wheel events as SGR mouse sequences) whenever the active app has opted into mouse tracking, so `vim`, `less`, `htop`, `tmux`, and the like still receive raw wheel events.

## ✨ New: Linux release pipeline in GitHub Actions

A new workflow (`.github/workflows/release-linux.yml`) builds `.deb` and `.rpm` artifacts on every `v*` tag push and attaches them directly to the draft GitHub Release. Runs on `ubuntu-22.04` so the binaries link against an older glibc and are usable on most current Linux distros without rebuilding.

## 📝 New: CHANGELOG.md

Project history is now tracked formally in `CHANGELOG.md` following the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) convention, with retroactive entries for 0.2.1 / 0.2.0 / 0.1.0.

---

# Release Notes — v0.2.1 (2026-05-17)

A maintenance release with two bug fixes, a long-overdue About panel, and a bundle identifier change.

## 🐛 Fixed: Mouse-wheel scroll not reaching the bottom

In some terminals, after scrolling up with the mouse wheel you couldn't fully scroll back down — the viewport would stop a few pixels short and only resync when you typed a key. This was the long-standing xterm.js wheel-drift bug ([xtermjs/xterm.js#4959](https://github.com/xtermjs/xterm.js/issues/4959)): the viewport's `scrollTop` lands 1px above the true bottom, leaving the terminal's internal "user is scrolled up" flag set, so incoming output is withheld from auto-scroll.

TermPane now hooks the viewport's wheel event: when you scroll downward and end up within one row of the bottom, it snaps to the true bottom. Auto-scroll for incoming output is restored, and reading scrollback (scrolls that stop well above the bottom) is unaffected.

## 🐛 Fixed: "Split vertical" / "Split horizontal" labels were swapped

In the bottom shortcut bar, the labels for `⌘D` and `⌘⇧D` were reversed relative to the standard terminal-multiplexer convention. `⌘D` (panes side-by-side, vertical divider) now reads "split vertical", and `⌘⇧D` (panes stacked, horizontal divider) reads "split horizontal". Only the labels changed — keybindings and split behavior are unchanged.

## ✨ New: Populated About panel

The macOS **TermPane → About TermPane** menu now opens a real About panel with the app name, version, icon, project website (`termpane.com`), and license (PolyForm Internal Use 1.0.0). Previously the panel was effectively empty because no custom `AboutMetadata` had been wired into the menu.

Implementing this required replacing the implicit default app menu with an explicit one built in Rust, which also gives the standard **Edit**, **View** (fullscreen), and **Window** submenus their correct items on macOS.

---

# Release Notes — v0.2.0 (2026-05-16)

Six user-visible improvements landed today.

## ✨ New: Per-card memory usage

Each terminal card in the sidebar now shows the live memory footprint of its shell and every descendant process, refreshed about four times per second. The value appears as a small, muted label aligned to the right of the foreground command, and tucks itself away on hover so the close button and creation time still get their own slot.

Under the hood this is a single batched `panes:stats` event per monitor tick — one entry per pane — that walks the parent/children index built from `sysinfo` once per tick and sums RSS by BFS from the pane's shell PID.

## 🐛 Fixed: Accented characters appearing as `c`

Typing `á`, `é`, `í`, `ó`, `ú`, `â`, `ê`, `ô`, `ã`, `õ`, or `ç` no longer produces a stray `c` in the terminal. Diacritics, dead-key composition, and TUIs (vim, less, fzf) all handle accented input correctly now.

The root cause was a locale issue: macOS GUI apps launched through `launchd` do not inherit `LANG`/`LC_*` from the user's shell login files. Without a UTF-8 locale, zsh's ZLE in emacs keymap (which TermPane forces via its shell-integration shim) interpreted high-bit bytes as the leading byte of a meta sequence and stripped bit 7 on input — which deterministically turned every UTF-8 byte `0xC3` (the first byte of every accented Latin character) into `0x43`, i.e. `C`. TermPane now detects the user's preferred locale (via `defaults read -g AppleLocale` on macOS, fallback `en_US.UTF-8`; `C.UTF-8` on Linux) and injects both `LANG` and `LC_CTYPE` before spawning the shell, unless the inherited environment already has a UTF-8 locale.

## ✨ New: Collapsible & resizable sidebar

The sidebar can now be hidden to free up screen real estate, and its width is adjustable.

- Click the chevron at the right of the sidebar header to collapse.
- When collapsed, a small chevron button at the top-left of the terminal area expands it again.
- Drag the right edge of the sidebar to resize it (clamped between 180 and 500 px).
- `⌘B` toggles the sidebar from anywhere.

Both the collapsed state and the chosen width persist across restarts (localStorage). The terminal grid re-fits xterm automatically as the layout changes.

## ✨ New: Extract a pane into its own card

When a terminal has been split into multiple panes, you can now "promote" one of them into a standalone card in the sidebar.

- Focus the pane you want to extract.
- Press `⌘⇧E`.

The split shrinks (or disappears if it had only two panes), and a new card appears immediately below the source in the sidebar with the moved pane as its sole occupant. The pane's PTY, shell, foreground process, scrollback, and output stream are all preserved — nothing restarts. The shortcut is a no-op when the active terminal has only one pane.

## ✨ New: New terminals inherit the active shell's working directory

Opening a new card with `⌘T` no longer dumps you in `$HOME`. The new shell starts in the same directory the active pane is currently `cd`'d into, so you can branch off your work without re-typing the path. Splits (`⌘D` / `⌘⇧D`) follow the same rule — the new pane inherits the cwd of the pane being split.

The cwd is resolved from the live shell process (via `sysinfo`), not from a cached value, so it always reflects the current directory of the prompt even if a foreground command is running. The first card created at app startup still opens in `$HOME` (there's no active shell to inherit from yet). If the lookup fails for any reason — permissions, dead shell, exotic platform — TermPane falls back silently to the previous behavior.

## ✨ New: Confirm before quitting when something is running

Closing the window (the red traffic-light button on macOS) used to terminate every shell and any commands they were running with no warning. Now, if any pane currently has a foreground process, TermPane intercepts the close and shows a confirmation dialog listing exactly which terminals have something running — so a stray click doesn't kill an in-flight build, a long-running script, or a vim session with unsaved buffers. When all shells are idle (just sitting at a prompt), the window closes immediately as before.

Under the hood: the backend listens for `WindowEvent::CloseRequested`, scans every pane's tracked foreground command, and calls `api.prevent_close()` + emits `app:close-requested` when the list is non-empty. The frontend renders a `ConfirmDialog` and calls a new `quit_app` command on confirm, which `WebviewWindow::destroy()`s the window directly to bypass the interceptor.

## Keyboard shortcuts added this release

| Shortcut | Action |
|---|---|
| ⌘B | Toggle sidebar |
| ⌘⇧E | Extract active pane to a new card |
