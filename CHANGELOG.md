# Changelog

All notable changes to TermPane are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-10-24

### Fixed

- **"Needs permission" for things Claude approved by itself.** The plugin
  flagged every permission-check `ask`, but in auto mode most asks go to
  Claude's classifier and never show a dialog. It now sends whether the ask was
  forced (`verdict.rule` / `hook` / `setting` — an ask rule, a hook, a read
  outside the working folders), and TermPane reads the session's mode from the
  transcript's latest `permission-mode` entry (`session_permission_mode`):
  auto / bypassPermissions keep only forced asks, dontAsk none, other modes
  all (`permission_dialog_likely` in `claude.rs`). Verified end to end in auto
  mode: an approved `mkdir` no longer flags the tab; an ask rule's dialog
  does. `AskUserQuestion` now reports "waiting for you". (The footer's
  `SessionMode` render props turned out empty, so the mode can't be read
  there.)

- **`claude` in a TermPane shell treated as a child session** (transcript
  saving off) when TermPane itself was launched from inside a Claude Code
  session. `spawn_shell` now drops Claude Code's per-session runtime markers
  (`CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_SESSION_ID`,
  messaging socket/token, …; `is_claude_session_marker` in `pty/spawn.rs`)
  from the inherited environment with `CommandBuilder::env_remove` (the builder
  starts from a copy of the process environment, so skipping them isn't
  enough); user-level Claude/Anthropic settings are kept.
- **"TermPane is damaged and can't be opened" on other Macs.** Only the binary
  carried the linker's ad-hoc signature; the `.app` bundle itself was unsigned,
  which Gatekeeper reports as *damaged* for a downloaded (quarantined) copy.
  `bundle.macOS.signingIdentity: "-"` in `tauri.conf.json` now ad-hoc signs the
  whole bundle (hardened runtime), so `codesign --verify --deep --strict` passes
  and a downloaded copy gets the regular "unidentified developer" prompt instead.

- **Panes coming back black after hibernation.** The wake path recreated WebGL
  for every pane at once and too early. Recovery is now split: window focus only
  refits and repaints (`recover(false)`); a system wake / page becoming visible
  drops the WebGL context and paints with the DOM renderer for 1.5 s before
  re-acquiring it, since a context created before the GPU is ready dies again
  (`recover(true)` in `TerminalView.svelte`, `recoverAllPanes(hard)`). If a
  window still ends up blank, View → Redraw Terminals reloads it losslessly.

- **Window flashing black for a few seconds with many terminals open.** Every
  pane of every card (all cards stay mounted) loaded its own `WebglAddon`, and
  WebKit caps live WebGL contexts per page (~16), force-losing the oldest past
  that. xterm waits ~3 s for a restore, falls back to the DOM renderer, then
  retried WebGL after 1 s — evicting another context, in a loop. The
  focus/visibility `recoverAllPanes()` sweep also recreated every context at
  once. Now a pane holds a WebGL context only while its terminal is the visible
  one (released on hide, re-acquired a tick after show so the outgoing terminal
  frees first), with a module-level cap of 8 live contexts; anything past the
  cap stays on the DOM renderer. Frontend: `TerminalView.svelte`
  (`MAX_WEBGL_CONTEXTS`, `releaseWebgl()`, `terminalActive` effect).

- **White/blank window after the Mac wakes from sleep or hibernation.**
  Recovery is now layered, because the wake failure has two very different
  modes. **(1) In-page repaint** (JS heap still alive — GPU context loss, a
  stalled compositor): WKWebView surfaces display sleep/hibernate as a Page
  Visibility change, so on `visibilitychange → visible` (and on window `focus`)
  every mounted pane recreates its WebGL context from scratch, re-fits, and
  forces a full repaint (`term.refresh`). Frontend: `recover()` in
  `TerminalView.svelte` (WebGL addon hoisted to a component field so it can be
  disposed on demand), `recoverAllPanes()` in `paneActions.ts`. **(2) Native
  reload** (the decisive fix — WKWebView's WebContent process was *terminated*
  during hibernation, so the JS heap is gone and no in-page code can run): a new
  Rust module `wake.rs` registers an `NSWorkspaceDidWakeNotification` observer
  (`objc2`/`objc2-app-kit`) and emits `system:woke`. The frontend answers from
  inside a `requestAnimationFrame` — which only fires once the compositor paints
  again — by running the repaint and calling the `ack_woke` command. A native
  watchdog thread waits ~2.5 s for that ack; if none arrives (the webview is
  genuinely dead), it calls `-[WKWebView reload]` (`objc2-web-kit`), relaunching
  the renderer. The PTYs live in the Rust process, so a reload brings the
  terminals back attached to the same live shells — only on-screen scrollback is
  lost, and only in the unrecoverable case. macOS-only; the `objc2-*`/`block2`
  crates were already in the tree via wry, so no new build weight.

### Changed

- **Status bar: one "Keyboard shortcuts" button instead of a row of hints.**
  The hint row had grown past readable. All shortcuts now live in a grouped
  dialog (`KeyboardShortcutsDialog.svelte`, data in `src/lib/keyboardShortcuts.ts`)
  opened from that button or **Help → Keyboard Shortcuts** (⌘? /
  `Ctrl+Shift+Alt+/`; `SHORTCUTS_ID` in `menu.rs` → `help:shortcuts` event).

- **Terminals opened from a custom shortcut are labeled with just the shortcut
  name** (e.g. `MyServer`) instead of `MyServer#<seq>`. The `#<seq>` suffix is
  still used for plain `+ New terminal` cards (`Term#N`). Labels are
  display-only, so repeated opens sharing a name is fine. Backend:
  `open_terminal_impl` in `commands.rs` (named → verbatim label; unnamed →
  `allocate_label("Term")`).

### Added

- **Claude Code integration: live status per tab.** New CLI modes of the same
  binary, dispatched from `main.rs` before any GUI setup (`cli.rs`):
  `termpane hook` reads a Claude Code hook payload on stdin and forwards a slimmed
  copy to the owning instance; it always exits 0 silently. Every spawned shell
  gets `TERMPANE_PANE_ID`, `TERMPANE_SOCKET` and `TERMPANE_BIN` (`spawn_shell`
  takes `extra_env`).
  Each instance serves a user-only Unix socket at
  `<data dir>/TermPane/run/<pid>.sock` (`control.rs`; falls back to
  `$TMPDIR/termpane-<uid>/` if the path would exceed `sun_path`), one JSON line
  in / one out. Hook events (`SessionStart`, `UserPromptSubmit`, `PreToolUse`,
  `PostToolUse`, `Notification`, `Stop`, `SessionEnd`) update a per-pane
  `ClaudeInfo` (`model.rs`/`claude.rs`: status, tool detail, Claude's own cwd,
  files written) emitted as `pane:claude`. The card shows a spinner while Claude
  works, an amber dot + text for "needs permission", and the current tool
  (`Bash: npm test`, `Edit state.rs`). Permission / waiting / done flag the card
  like a bell. The monitor marks the session ended once the pane's shell is back
  at its prompt.
- **TermPane's Claude Code plugin, installed in one click.** Everything
  Claude-side ships as one plugin, `integrations/claude-code/plugin/` (a folder
  marketplace `termpane` with plugin `termpane`; a function-hooks module,
  `hooks/register.ts`). It reports `session.start` / `session.end`,
  `turn.start` / `turn.complete` (main loop), `tool.call` (pre + post) and
  `tool.check` asks put to a person (permission) to TermPane through
  `termpane hook`, in the settings-hook payload shape `claude.rs` reads — not
  through `classic.*` events, which an organization's managed plugin seated
  above may pass on without running the hooks beneath it (observed with
  `cc-plugin-sec-default`). It registers the tab tools as
  `mcp__termpane__<name>`, served by `termpane tool <name> <json>` (specs from
  `termpane tool --list`, so they can't drift). Outside TermPane
  (`TERMPANE_BIN` unset) it does nothing. Preferences → *Claude Code
  integration* → **Install Claude Code plugin** writes the embedded files
  (`include_str!`) to `<data dir>/TermPane/claude-plugin/`, runs `claude plugin
  marketplace add` + `claude plugin install termpane@termpane` through the
  login shell, and removes the earlier settings-based setup (TermPane's hook
  entries in `~/.claude/settings.json`, backup + atomic; the `termpane` MCP
  server) so nothing reports twice. Commands `claude_integration_status`,
  `install_claude_plugin`.
- **`/fork` opens the copy in a TermPane sub-tab.** Inside TermPane the
  plugin answers the built-in `/fork` (`command.run`): `termpane fork --session
  <id> --prompt <args>` asks the instance (control op `fork`) to open a sub-tab
  nested under the current tab, named `FORK - <tab>`, in Claude's working
  directory, running `claude --resume <id> --fork-session -n 'FORK - <tab>'`
  (plus the `/fork` prompt, if any), and brings it to the front. Outside
  TermPane, or if TermPane can't do it, the built-in `/fork` runs as usual.
- **New tabs follow Claude into worktrees.** New tabs, sub-tabs and splits
  start in Claude's cwd (from hook payloads) while Claude runs in the source
  pane, else the shell's (`pane_cwd` in `commands.rs`).
- **Nested sub-tabs.** `Terminal.parent_id`; the list is kept in depth-first
  order (`normalize_tree_order`, `subtree_end` in `state.rs`) so it doubles as
  the display order. ⌘⇧T / `Ctrl+Shift+Alt+T` (or the card's context menu)
  opens a sub-tab of the active tab in its working directory. Cards indent by
  depth and collapse their sub-tabs (chevron; `collapsedTabs` in
  `sidebarLayout.ts`). Dragging a card moves its whole subtree and adopts the
  parent of the card it lands above (`move_terminal` command). Closing or
  merging a tab promotes its sub-tabs one level instead of closing them.
- **Files changed per Claude session + safe stage.** `PostToolUse` of
  `Edit`/`Write`/`MultiEdit`/`NotebookEdit` records the file per session — not
  `git status`, so several sessions sharing one directory stay separate. Cards
  show a pencil badge with the count; `FilesDialog.svelte` lists files per
  session, flags files another session in this window also wrote, and **Stage**
  runs `git add -A --` per repository on the exclusive files only (⌘⇧S /
  `Ctrl+Shift+Alt+S` stages the active tab's). Commands `get_terminal_files`,
  `stage_terminal_files`, `clear_terminal_files`.
- **Claude can control TermPane tabs.** Tools `list_tabs` (with each session's
  status and changed files), `read_output`, `send_input`, `open_tab` (optionally
  as a sub-tab), `focus_tab`, `close_tab`, working across every running
  instance by enumerating the sockets (stale ones are removed). Served to Claude
  by the plugin; `termpane mcp` also exposes them as a stdio MCP server for
  other agents. `read_output`
  replays the pane's backlog through a `vt100` emulator at the pane's width, so
  line editing, cursor moves and progress bars read as they looked on screen.
- **Resume a Claude session.** Sidebar button / ⌘⇧O (`Ctrl+Shift+Alt+O`) opens
  `ResumeDialog.svelte`, listing sessions from `~/.claude/projects/*/*.jsonl`
  (read-only; head for start cwd + first prompt, tail for `custom-title` /
  `ai-title` / branch; cached by mtime + size; newest 300), filtered to the
  active project by default, with search. Picking one opens a tab in the
  session's folder running `claude --resume <id>`. Command
  `list_claude_sessions`, `active_cwd`.
- **Reopen tabs after a quit or crash.** `restore.rs`: each instance rewrites
  `sessions/<pid>.json` (tabs, sub-tabs, splits, ratios, cwds, unread flags,
  live Claude session ids) when it changes, holding an exclusive `flock` on
  `sessions/<pid>.lock`. On launch, files whose lock is free belong to dead
  instances and are restored into this window, then deleted — a live second
  window is never duplicated. Panes that ran Claude get `claude --resume <id>`.
  New `restoreSessions` preference (default on).
- **Mark a tab as unread.** `Terminal.marked_unread` (persisted by the restore
  file): hollow ring + bold label until cleared by hand. ⌘⇧U /
  `Ctrl+Shift+Alt+U` or the context menu. ⌘J / `Ctrl+Shift+J` jumps to the next
  tab that is marked unread, flagged by a bell, or has Claude waiting.
- **Jump between your prompts to Claude.** While Claude Code runs in a pane,
  ▲/▼ buttons on its right edge (and ⌘↑/⌘↓, `Ctrl+Shift+↑/↓`) scroll to the
  previous / next prompt you typed, put it one line below the top and flash it.
  Prompts are found by scanning the xterm buffer for Claude's `❯ ` transcript
  lines, skipping its live input box (under a `─` rule) and numbered choices
  (`❯ 1. Yes`) — no plugin needed (`promptLines` / `jumpToPrompt` in
  `TerminalView.svelte`, `jumpToPrompt` in `paneActions`). The same column has
  go-to-start / go-to-end buttons at its top and bottom (⌘Home / ⌘End,
  `Ctrl+Shift+Home/End`; `scrollToEdge`).
- **Calmer tab cards with an info balloon.** The card's second line is only
  the branch / worktree; servers and their links live in the balloon alone, so
  clicking a tab never opens the browser by accident. Hovering changes nothing in place: memory stays, the × has a
  reserved slot and only becomes visible, the open time moved out. After 2 s
  of hover, or at once from a discreet ⓘ, a balloon (`CardInfo.svelte`)
  shows the full name, what Claude or the command is doing, the folder with a
  copy button, git, every server with its link, the files Claude changed with
  **Review & stage**, open time, memory and pane count. Pane context now carries
  `cwd`. The sub-tab chevron is larger and full-contrast.
- **Works with Dev Containers.** New `devcontainer.rs`: a tab's project
  configuration is found walking up from its cwd — `.devcontainer/devcontainer.json`,
  `.devcontainer.json` or `.devcontainer/<name>/devcontainer.json`, never above
  the git checkout's root so a worktree doesn't take the main checkout's — and
  read as JSONC (`strip_jsonc`: comments, trailing commas). Its
  `portsAttributes` name the card's ports (`label`), set their scheme
  (`protocol`) and hide `onAutoForward: "ignore"` ones; the spec's
  tool-specific block `customizations.termpane.portUrls` gives the exact
  address to open (another host, a path). Ports are now `PortInfo { address,
  label, url }`, built by `context::describe_ports` (databases/caches keep no
  url). A pane menu item **Open shell in dev container** (`open_devcontainer_shell`)
  opens a sub-tab running `docker compose -f … exec -w <workspaceFolder>
  <service>`, starting the service only when it isn't running (never
  recreating one another tool owns). Card links now go through
  `open_port_url`, which opens only a url TermPane derived for that tab, so the
  opener capability no longer lists loopback hosts.
- **Copy buttons on Claude's code blocks.** When a turn ends (or a session
  starts), TermPane reads the messages added to the session transcript since
  its last scan (`collect_code_blocks`, offset per session; a first-seen session
  from its last 1 MB) and records each fenced block verbatim
  (`fenced_blocks`: ``` / ~~~, optional language, a list indent removed;
  `ClaudeInfo.code_blocks`, newest 60). `TerminalView` finds each block on
  screen by its first line of 6+ characters (latest unused occurrence) and puts
  a copy button on its top row's right edge as an xterm decoration; a `{ }`
  pane button lists every block for the ones it can't place. Copies use
  `copyToClipboard` (`src/lib/clipboard.ts`: Clipboard API, `execCommand`
  fallback outside the pane). The copy is the markdown source, so indentation
  and line breaks the TUI rendering loses are kept.
- **Copying a long command gives one line.** Claude Code (and other TUIs)
  wrap long lines with real newlines, so a wrapped command used to copy as
  several lines. ⌘C now joins rows a program wrapped: a break counts as a wrap
  when the next row's first word could not have fitted on the row above
  (`wrappedByApp` / `joinRows` in `src/lib/copyText.ts`), the continuation's
  indent is dropped and a space restored unless the break cut through a
  single token — a long path, URL, ARN or JSON blob, recognised because its two
  halves together are longer than a row (`splitMidToken`); a row ending in a
  shell continuation (` \`) is never joined; the wrap width is estimated from
  the longest nearby row (`wrapWidth`, box rules excluded), because a
  program's hard breaks keep the width they were printed at when the window is
  widened later; xterm's own soft wraps and real line breaks behave as before. ⌘⇧C
  (`Ctrl+Shift+Alt+C`) copies exactly as displayed. A capture-phase `copy`
  listener in `TerminalView.svelte` writes the clipboard.
- **Dock badge** with the number of tabs that need you (flagged by a bell /
  notification while unwatched, or Claude asking for permission or input):
  `set_badge_count` command, `$derived` count + effect in `+page.svelte`.
- **Tabs named after their Claude session.** A tab follows the title of the
  Claude session running in it (`Terminal.auto_label`, persisted by restore)
  until the user renames it — whatever opened it: `+ New terminal`, any
  custom shortcut (its name only says what opened the tab), a resumed
  session — the title being — its custom title, else the one Claude
  generates — shortened to whole words of ~28 characters (`short_label`),
  re-checked on `SessionStart` / `Stop` (`follow_session_title`, reading the
  transcript tail; `transcript_path` finds `~/.claude/projects/*/<id>.jsonl`).
  Picking a session in `claude --resume` renames the tab within seconds
  (verified end to end). Only a rename by the user and a fork's `FORK - …`
  stay put; a tab where Claude never runs keeps its name anyway.
- **Renaming a tab names its Claude sessions.** `rename_terminal` pins the
  label and appends a `custom-title` entry — what `/rename` writes — with a
  longer form for the /resume picker: `<label> — <repo>/<branch>` or
  `<label> — <repo> (worktree <name>)` (`name_sessions_after_tab`). A running
  session shows it in its prompt box from its next start; mods have no API to
  rename the live session.
- **Branch / worktree and open ports on tab cards.** A second line under the
  name shows `main` or `worktree: feat-x` and the addresses the tab's
  processes listen on (`127.0.0.1:5173`), loopback ones clickable to open in
  the browser. New `context.rs`: `git rev-parse` per directory (cached 10 s;
  worktree when `--git-dir` ≠ `--git-common-dir`) and one `lsof -iTCP
  -sTCP:LISTEN` for the machine, mapped to each pane's process subtree; the
  monitor refreshes them every 12 ticks (~3 s) and emits `pane:context` on
  change. `PaneSnapshot` gained `git` / `ports`. Ports skip Claude Code's own
  helper socket and the OS's ephemeral range (49152+), and add the host ports
  of Docker Compose containers whose project directory
  (`com.docker.compose.project.working_dir`) is the most specific one
  containing the tab's cwd — the repo's containers for a tab at its root, a
  worktree's for a tab in that worktree (`DockerPorts`, polled every 10 s on a
  thread of its own so a wedged Docker can't stall the monitor). Database /
  cache ports are listed but not linked. The opener capability now
  allows `http://127.0.0.1:*`, `http://localhost:*` and `https://claude.ai/*`.
- **Diff per session.** Each file in the Files dialog has a **Diff** toggle
  (`file_diff`: `git diff HEAD -- <file>`, or `--no-index` against
  `/dev/null` for a new untracked file), colored by line, noting when another
  tab also edited the file.
- **Clickable file paths.** ⌘-click (Ctrl-click) `src/app.ts:42:7`-style paths
  in any pane to open them in your editor: an xterm link provider
  (`registerPathLinks` in `TerminalView.svelte`; a bare name needs a line
  number, URLs stay with the web-links addon) → `open_path`, which resolves
  relative paths against the pane's cwd and runs the new **Open file paths
  with** preference (`editorCommand`, `{file}` / `{line}` / `{col}`; empty =
  `code -g`, then the system default app).
- **List of your prompts.** A list button in the prompt column (⌘⇧L /
  `Ctrl+Shift+Alt+L`) opens every prompt typed to Claude in the pane, with its
  wrapped text; click one to jump there.
- **Find in scrollback.** ⌘F (`Ctrl+Shift+F`) opens a search bar on the active
  pane (`@xterm/addon-search` 0.15): incremental, Enter / ⇧Enter or ⌘G / ⌘⇧G
  for next / previous, match count, Esc to close.
- **See what Claude runs in the background.** The plugin reports backgrounded
  Bash commands (`backgroundTaskId`) and background subagents
  (`status: "async_launched"`) as `TermPaneBackground` events
  (`ClaudeInfo.background`). A spy button on the pane opens
  `BackgroundDialog.svelte`: view-only, the task list beside its output,
  refreshed every second and following the end while you're there.
  `read_background_output` finds `<temp>/claude-<uid>/<project>/<session>/tasks/<id>.output`
  (or the path Claude reported), renders command output through the `vt100`
  reader and a subagent's JSONL transcript as prompts, text, tool calls and
  result heads (`render_agent_transcript`).
- **Artifacts from the session.** The plugin reports each published Artifact
  (`TermPaneArtifact`, `ClaudeInfo.artifacts`); a pane button lists them by
  title, opening `https://claude.ai/artifact/…` in the browser.
- **The installed plugin follows TermPane updates.** On launch, if the plugin
  was installed, its files are rewritten from the embedded copy when they
  differ (`refresh_installed_plugin`).
- **⋯ menu on every pane, kept short.** Only what belongs to the pane, in
  titled groups (`section` in `TabContextMenu.svelte`): **Claude** (fork the
  session into a sub-tab — `fork_claude_session`, same path as `/fork` — files
  changed, list of prompts), **Layout** (split side by side / stacked; flip and
  move to its own tab with 2+ panes) and **Close**. Tab actions stay on the
  card's right-click menu, which gained **Open shell in dev container**;
  find-in-scrollback is a magnifier button beside ⋯; prompt / start / end
  navigation, background tasks and artifacts keep their own buttons; clear
  stays on ⌘K and redraw in the View menu. Dialogs and the close confirmation
  are requested through `src/lib/uiRequests.ts` and handled in `+page.svelte`.
- **Card context menu** (right-click): new sub-tab, mark read/unread, files
  changed by Claude, move to top level, close (`TabContextMenu.svelte`).
- **View → Redraw Terminals** (⌘⇧R / `Ctrl+Shift+Alt+R`): reloads the webview
  without losing scrollback — every pane keeps a 1 MB in-memory backlog
  (`pty/backlog.rs`) that a freshly mounted `TerminalView` replays
  (`get_pane_backlog`); `pty:output:<pane>` now carries `{ o, d }` (stream offset
  + base64) so chunks already covered by the replay are dropped. Also fixes the
  first bytes of a new pane being lost before its listener attached.
- **Reorder custom shortcuts.** The Custom terminals dialog
  (`ShortcutsDialog.svelte`) now has ↑/↓ buttons on each row to move it up or
  down; the new order is persisted on Save and drives the order of the
  `+ custom` dropdown. No backend change — `Shortcuts.items` was already an
  ordered list preserved through `set_shortcuts`/`sanitize`.

## [0.3.1] - 2026-06-26

### Fixed

- **Blank terminal pane after waking the Mac from sleep.** When macOS tears
  down the GPU's WebGL context on wake from sleep/hibernation, the affected
  pane could go blank until it was resized or recreated. TermPane now listens
  for the WebGL renderer's context-loss event, disposes the dead addon so
  xterm falls back to its DOM renderer (the pane keeps drawing), re-fits the
  viewport, and retries WebGL once the GPU settles. Retries are gated by a
  **consecutive**-failure counter that resets on every successful load, so a
  long-lived pane that recovers cleanly across many sleep/wake cycles is never
  permanently demoted to the DOM renderer, while a genuinely unavailable GPU
  stops churning after four attempts instead of looping forever. This is a
  fresh, more conservative take on the `onContextLoss` handling that was
  reverted in 0.3.0 — it relies on xterm's own DOM-renderer fallback rather
  than the canvas-renderer swap and atlas clears that degraded rendering then.

- **Stale frontend served after an app update.** WKWebView caches the bundled
  frontend per origin and kept serving the previous build's HTML/CSS/JS after
  an update, so changes (e.g. the new scrollbar) never appeared even though the
  binary shipped them. On launch TermPane now compares the running version to
  the last one recorded in `state.json` and, on a mismatch, purges the on-disk
  WebKit HTTP cache before the webview loads, so it refetches the freshly
  embedded assets. The purge targets only the cache directory
  (`~/Library/Caches/com.termpane`); localStorage lives in a separate store, so
  user preferences (theme, sidebar width, …) are preserved across updates.

### Added

- **Open a second window (separate instance).** A new **File → New Window**
  menu item (`⌘N` on macOS, `Ctrl+Shift+N` on Linux) launches an independent
  TermPane instance with its own terminals. On a packaged macOS build it goes
  through LaunchServices (`open -n <bundle>`) so the new instance registers as
  a proper separate app; in dev / unpackaged builds it re-execs the binary via
  `std::env::current_exe()`. Each instance owns its own in-memory workspace,
  while sharing the on-disk `state.json` / `preferences.json` (atomic writes,
  so no corruption — last writer wins). Backend: `menu::spawn_new_instance`,
  wired through `on_menu_event` in `lib.rs`.

- **Styled vertical scrollbar in the terminal viewport.** Terminal panes now
  render a themed 12px scrollbar (transparent track, rounded thumb derived from
  the foreground color that brightens on hover) instead of the platform
  default. On macOS the native overlay scrollbar is disabled
  (`-webkit-appearance: none`) so the custom style actually renders, and the
  thumb keeps usable contrast in both light and dark themes.

- **Switch terminal cards from the keyboard.** New shortcuts cycle the active
  card without reaching for the mouse: `⌘⇧→` / `⌘⇧←` on macOS, and
  `Ctrl+Shift+Alt+→` / `Ctrl+Shift+Alt+←` on Linux (following the established
  app-mod convention). Selection wraps around at both ends and is a no-op when
  fewer than two terminals are open. Routes through the same `setActiveTerminal`
  path as clicking a card, so backend focus state stays in sync.

- **Attention indicator when a terminal rings the bell.** When a program emits
  the terminal bell (`\x07`) — e.g. Claude Code finishing a turn and waiting for
  input — the owning sidebar card pulses a dot for ~3s as immediate feedback.
  If the bell rang on a terminal you weren't watching (not the active card, or
  the window was unfocused), the card additionally keeps a **persistent dot plus
  a faint accent highlight** until you open that terminal, so you can tell at a
  glance which one wants you. When TermPane itself isn't the focused window, it
  also fires a **native OS notification** (macOS Notification Center via
  `tauri-plugin-notification`), throttled to one per terminal until you visit
  it. The notification is **on by default** and can be turned off in Preferences
  (the `bellNotifications` pref); the card pulse/badge is always shown
  regardless. Implementation: `term.onBell` in `TerminalView` routes to
  `signalBell` in the new `src/lib/attention.ts` store (ephemeral per-terminal
  `{ nonce, persistent }` map, like broadcast state); the card reads it in
  `TerminalCard`; `+page.svelte` clears it on activation and on window refocus.
  On macOS the OS notification needs the standard notification permission:
  TermPane requests it on the first background alert (`ensurePermission()` in
  `attention.ts`) and, while still ungranted, re-checks `isPermissionGranted()`
  before every later alert — so granting it afterwards in **System Settings →
  Notifications → TermPane** takes effect with no restart (the prompt itself is
  shown at most once). The app only appears in that Notifications list after it
  has attempted to notify at least once.

- **`OSC 9` / `OSC 777` desktop-notification sequences.** TermPane now parses the
  iTerm2/Ghostty/VS Code-style notification escape codes (`ESC ] 9 ; msg` and
  `ESC ] 777 ; notify ; title ; body`) and routes them through the same
  attention path as the bell, carrying the tool's message into the OS
  notification. Lets any tool that emits these — including Claude Code's
  `iterm2` notification channel — light up a card with no hook. Implementation:
  `term.parser.registerOscHandler(9 / 777, …)` in `TerminalView` →
  `signalNotification` in `attention.ts` (OSC 9 skips ConEmu's `9;4` progress
  sub-command).

- **Claude Code notification bridge (`integrations/claude-code/`).** A versioned
  hook script (`termpane-bell.sh`) plus a setup guide. Claude Code notifies
  through the OS rather than ringing the terminal bell, so it doesn't reach
  TermPane on its own; the hook rings the bell on the Claude process's
  controlling tty (walking the process tree, since hooks run detached from
  `/dev/tty`), scoped to TermPane via `TERM_PROGRAM`. Wired into Claude's `Stop`
  and `Notification` hooks. README documents both the hook and the simpler
  `preferredNotifChannel: "terminal_bell"` alternative.

- **Custom terminal shortcuts (`+ custom`).** A new **+ custom** button in the
  sidebar footer opens a dropdown of user-defined named shortcuts, each opening a
  terminal pre-configured with an optional working directory, startup command,
  and shell override. Clicking a shortcut spawns the terminal — the command is
  typed into the interactive login shell (sent from the frontend once the pane
  has subscribed to output, so its echo and early output aren't lost), so the
  terminal stays alive when it exits (e.g. an `ssh` session disconnecting drops
  you back to a prompt) — and the new card is labelled `<shortcut name>#<seq>`
  instead of `Term#<seq>`,
  reusing the same label counter. A **Manage shortcuts…** item opens a CRUD editor
  rendered as an accordion: each row shows only the name, expand it to edit
  directory / command / shell, with add/remove. Shortcuts persist in
  `shortcuts.json` (atomic write; `sanitize` trims names, drops nameless items,
  collapses blank optionals). A leading `~` in the directory is expanded to
  `$HOME`, and a path that isn't an existing directory is ignored (falls back to
  the active pane's cwd). Backend: new `shortcuts.rs` module + `get_shortcuts` /
  `set_shortcuts` commands (emit `shortcuts:changed`); `open_terminal` gained
  optional `cwd` / `command` / `shell` / `name` args; the startup command is
  stashed on the PTY and flushed via a new `take_startup_command` command;
  `spawn_shell` gained a shell override; `allocate_label` takes a label prefix
  and persists the counter under a serialized lock. Frontend: `src/lib/shortcuts.ts`
  store, `CustomTerminalMenu.svelte` dropdown, `ShortcutsDialog.svelte` editor.

## [0.3.0] - 2026-05-23

### Added

- **Drag-to-merge.** Drag a card from the sidebar onto another terminal's pane
  area to merge it as a split. Hold `Shift` while releasing to make the new
  split vertical instead of horizontal. The source terminal's entire layout
  subtree (single pane or nested splits) is preserved as a child of the new
  split node, then the source terminal is removed. PTYs keep running —
  identical guarantee to `extract_pane`. Backend:
  `AppState::merge_terminal_into_pane` + new
  `LayoutNode::split_at_with_subtree`. Frontend: pane host marked with
  `data-pane-id`; sidebar's existing pointer-drag resolves the target via
  `document.elementFromPoint` and publishes the current target through a new
  `$dropTargetPaneId` store (`src/lib/dragState.ts`).
- **Flip split orientation.** New shortcut `⌘/` (macOS) / `Ctrl+Shift+/`
  (Linux) flips the orientation of the split that immediately contains the
  active pane (horizontal ⇄ vertical). No-op when the pane is the sole leaf
  in its terminal. Backend: `LayoutNode::flip_parent_of` walks the tree from
  the leaf upward and toggles the nearest enclosing `SplitDir` via the new
  `SplitDir::flipped()` helper.
- **Broadcast input.** When the active terminal has 2 or more panes, a new
  toggle appears in the status bar. While on, every keystroke from any pane
  of that terminal is mirrored to every other pane in the same terminal
  (input only — output stays independent). The terminal gets a red outline
  while broadcast is active, and the mode auto-disables when the pane count
  drops below 2. Backend command `write_input_broadcast(terminal_id, data)`
  iterates over `layout.leaves()` and writes to each PTY in a single IPC
  round-trip. Frontend state lives in `$broadcastEnabled: Set<TerminalId>`
  (`src/lib/broadcast.ts`) — ephemeral, not persisted.
- **Linux keyboard shortcut layer.** Every macOS `⌘`-based app shortcut now
  has a Linux equivalent following the GNOME Terminal / Konsole convention:
  `Ctrl+Shift+letter` is the app-mod, leaving plain `Ctrl+letter` available
  as a PTY control byte. When a macOS shortcut adds Shift as a sub-modifier
  (e.g. `⌘⇧D`), the Linux mapping adds Alt instead
  (`Ctrl+Shift+Alt+D`), since Shift is already part of the base mod.
  Platform is detected via a new `get_platform` Tauri command (returns
  `std::env::consts::OS`) and exposed as `$platform`
  (`src/lib/platform.ts`). The status bar and a few tooltips render the
  correct labels for the current OS.

### Changed

- Status-bar shortcut hints now render per-platform: macOS shows `⌘T`, Linux
  shows `Ctrl+Shift+T`, and so on. The "Hide/Show sidebar" tooltips follow
  the same convention.

### Fixed

- **Right-click context menu suppressed outside terminal panes.** The
  WebView's default context menu (with a "Reload" item that would tear
  down every xterm.js instance and orphan its PTY in one accidental click)
  no longer appears in the sidebar, status bar, or any other chrome.
  Implementation: a global `contextmenu` listener that calls
  `preventDefault()` unless `e.target.closest('.xterm-host')` matches —
  inside terminal panes the native menu still appears so users can
  Copy/Paste/Inspect/etc.

### Reverted

- **v0.2.2 WebGL glyph atlas mitigations.** The three measures added in
  v0.2.2 — waiting for `document.fonts.ready` before opening the terminal,
  the `onContextLoss` listener with canvas fallback, and the
  `clearTextureAtlas()` calls on font/preference changes and on
  `cols`/`rows` changes — degraded rendering in practice (rather than
  fixing the glyph corruption they targeted) and are gone. The v0.2.2
  **mouse-wheel handler** (explicit `scrollLines()` with mouse-tracking
  passthrough) is independent and is kept.

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

[0.4.0]: https://github.com/ChavePreta/TermPane/releases/tag/v0.4.0
[0.3.1]: https://github.com/ChavePreta/TermPane/releases/tag/v0.3.1
[0.3.0]: https://github.com/ChavePreta/TermPane/releases/tag/v0.3.0
[0.2.2]: https://github.com/ChavePreta/TermPane/releases/tag/v0.2.2
[0.2.1]: https://github.com/ChavePreta/TermPane/releases/tag/v0.2.1
[0.2.0]: https://github.com/ChavePreta/TermPane/releases/tag/v0.2.0
[0.1.0]: https://github.com/ChavePreta/TermPane/releases/tag/v0.1.0
