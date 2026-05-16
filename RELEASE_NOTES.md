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
