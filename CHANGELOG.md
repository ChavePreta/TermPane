# Changelog

All notable changes to TermPane are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2] - 2026-05-20

### Fixed

- **WebGL glyph atlas corruption during long sessions with rich TUIs**
  (notably Claude Code). The `@xterm/addon-webgl` renderer would gradually
  desynchronize its glyph atlas under heavy redraw load, replacing on-screen
  text with garbled Unicode-like noise until a window resize forced a
  rebuild. TermPane now waits for `document.fonts.ready` before opening the
  terminal (so the atlas is built with correct metrics), listens for
  `onContextLoss` and falls back to the canvas renderer when the WebGL
  context is dropped, and calls `clearTextureAtlas()` on font/preference
  changes and on viewport `cols`/`rows` changes.
- **Mouse wheel scroll-down getting stuck after scrolling up** in sessions
  with heavy output. The previous handler relied on xterm's internal wheel
  routing, which fell out of sync under aggressive redraws. The wheel
  listener now lives on the host container and drives `term.scrollLines()`
  explicitly, deriving a sane line count from `fontSize × lineHeight`. It
  still bails out (and forwards as an SGR mouse sequence) when the running
  app has opted into mouse tracking, so `vim`, `less`, `htop`, etc. continue
  to receive raw wheel events.

### Added

- **Linux release pipeline.** GitHub Actions builds `.deb` and `.rpm`
  artifacts on every `v*` tag push and attaches them to the draft GitHub
  Release automatically (`.github/workflows/release-linux.yml`,
  `ubuntu-22.04` runner).

## [0.2.1] - 2026-05-17

### Added
- Native macOS **About TermPane** panel (app submenu) showing name, version,
  website, and license. Hand-built `Menu` replaces Tauri's implicit default
  while preserving standard Edit / View / Window shortcuts.

### Changed
- Status bar split labels swapped to match what the user sees:
  `⌘D` → "split vertical", `⌘⇧D` → "split horizontal". Behavior unchanged.
- Bundle identifier renamed from `com.zumpost.termpane` to `com.termpane`.

### Fixed
- Initial mitigation for the xterm.js mouse-wheel scroll-drift (viewport
  stopping 1px short of bottom and suppressing auto-scroll). Superseded by
  the more thorough fix in 0.2.2.

## [0.2.0] - 2026-05-16

### Added
- Per-card memory display: the sidebar card shows the foreground command's
  process tree RSS (parent shell + all descendants) updated each monitor
  tick, batched into a single `panes:stats` event.
- Collapsible + resizable sidebar. New `sidebarCollapsed` and `sidebarWidth`
  stores (localStorage-backed, width clamped 180–500 px). `⌘B` toggles.
- Pane extraction: `⌘⇧E` moves the active pane (PTY, reader thread,
  foreground state) into a brand-new card inserted right after the source,
  with no output interruption.

### Fixed
- UTF-8 locale handling for shells spawned by GUI apps on macOS. Accented
  vowels (`á`, `é`, `ç`, …) were being bit-7-stripped by zsh's emacs ZLE
  because launchd-spawned apps don't inherit `LANG` / `LC_*` from login
  shells. TermPane now reads `defaults read -g AppleLocale` on macOS (and
  falls back to `en_US.UTF-8`) or `C.UTF-8` on Linux, and injects `LANG` /
  `LC_CTYPE` before spawn unless the inherited env already has a UTF-8
  locale.

## [0.1.0] - 2026-05-15

Initial release.

- Tauri 2 + Rust + Svelte 5 (SvelteKit, adapter-static SPA) + xterm.js
  (`addon-webgl` + `addon-fit` + `addon-web-links`).
- `portable-pty` 0.9 backend.
- Multi-terminal sidebar with foreground command tracking
  (`tcgetpgrp` + `sysinfo`).
- Splits with recursive layout tree (horizontal / vertical, draggable
  splitter).
- Theme switcher (system / light / dark), preferences dialog
  (font family / size / line height / cursor blink).
- Drag-to-reorder, rename cards (double-click / F2), always-on-top pin.
- Keyboard shortcuts: `⌘T` new, `⌘W` close, `⌘D` / `⌘⇧D` splits, `⌘K` clear,
  `⌘=` / `⌘-` / `⌘0` font zoom, control-letter combos written as raw bytes
  to the PTY.
- macOS-first build (`.dmg` + `.app`); Linux `.deb` + `.AppImage` via local
  build only.

[0.2.2]: https://github.com/ChavePreta/TermPane/releases/tag/v0.2.2
[0.2.1]: https://github.com/ChavePreta/TermPane/releases/tag/v0.2.1
[0.2.0]: https://github.com/ChavePreta/TermPane/releases/tag/v0.2.0
[0.1.0]: https://github.com/ChavePreta/TermPane/releases/tag/v0.1.0
