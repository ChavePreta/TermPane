import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

export function openExternal(url: string): Promise<void> {
  return openUrl(url);
}

export type SplitDir = "horizontal" | "vertical";

export type LayoutSnapshot =
  | { type: "leaf"; id: string }
  | {
      type: "split";
      dir: SplitDir;
      children: LayoutSnapshot[];
      ratios: number[];
    };

export type ClaudeStatus =
  | "idle"
  | "thinking"
  | "tool"
  | "permission"
  | "waiting"
  | "done"
  | "ended";

export interface ClaudeInfo {
  sessionId: string;
  status: ClaudeStatus;
  detail: string | null;
  cwd: string | null;
  files: string[];
  background: BackgroundTask[];
  artifacts: ArtifactLink[];
  codeBlocks: CodeBlock[];
  updatedAt: string;
}

export interface CodeBlock {
  id: string;
  lang: string;
  code: string;
}

export interface BackgroundTask {
  id: string;
  kind: "bash" | "agent" | string;
  description: string;
  outputFile: string | null;
  startedAt: string;
}

export interface ArtifactLink {
  url: string;
  title: string;
}

export interface GitInfo {
  repo: string;
  branch: string;
  worktree: string | null;
}

export interface PortInfo {
  address: string;
  label: string | null;
  url: string | null;
}

export interface PaneContext {
  terminalId: string;
  paneId: string;
  git: GitInfo | null;
  ports: PortInfo[];
  devContainer: string | null;
  cwd: string | null;
}

export interface BackgroundOutput {
  text: string;
  path: string | null;
  active: boolean;
}

export interface PaneSnapshot {
  id: string;
  foregroundCommand: string | null;
  exitCode: number | null;
  claude: ClaudeInfo | null;
  git: GitInfo | null;
  ports: PortInfo[];
  devContainer: string | null;
  cwd: string | null;
  memoryBytes?: number;
}

export interface TerminalSnapshot {
  id: string;
  label: string;
  createdAt: string;
  layout: LayoutSnapshot;
  panes: PaneSnapshot[];
  activePane: string;
  parentId: string | null;
  markedUnread: boolean;
}

export interface PaneClaudeChanged {
  terminalId: string;
  paneId: string;
  claude: ClaudeInfo | null;
}

export interface SessionFiles {
  paneId: string;
  sessionId: string;
  live: boolean;
  files: string[];
  shared: string[];
}

export interface StageResult {
  staged: string[];
  shared: string[];
  errors: string[];
}

export interface ClaudeSession {
  id: string;
  title: string;
  cwd: string;
  gitBranch: string | null;
  startedAt: string | null;
  modifiedAt: string;
}

export interface IntegrationStatus {
  pluginInstalled: boolean;
  legacySetup: boolean;
  manualCommands: string;
}

export interface PaneForegroundChanged {
  terminalId: string;
  paneId: string;
  command: string | null;
}

export interface PaneStat {
  terminalId: string;
  paneId: string;
  memoryBytes: number;
}

export const api = {
  listTerminals: () => invoke<TerminalSnapshot[]>("list_terminals"),
  openTerminal: (opts?: {
    cwd?: string | null;
    command?: string | null;
    shell?: string | null;
    name?: string | null;
    parentId?: string | null;
    // Let the label follow the Claude session's title (e.g. a resumed session).
    autoLabel?: boolean;
  }) =>
    invoke<TerminalSnapshot>("open_terminal", {
      cwd: opts?.cwd ?? null,
      command: opts?.command ?? null,
      shell: opts?.shell ?? null,
      name: opts?.name ?? null,
      parentId: opts?.parentId ?? null,
      autoLabel: opts?.autoLabel ?? null,
    }),
  takeStartupCommand: (paneId: string) =>
    invoke<string | null>("take_startup_command", { paneId }),
  closeTerminal: (terminalId: string) =>
    invoke<void>("close_terminal", { terminalId }),
  splitPane: (paneId: string, direction: SplitDir) =>
    invoke<TerminalSnapshot>("split_pane", { paneId, direction }),
  closePane: (paneId: string) => invoke<void>("close_pane", { paneId }),
  focusPane: (paneId: string) => invoke<void>("focus_pane", { paneId }),
  writeInput: (paneId: string, data: string) =>
    invoke<void>("write_input", { paneId, data }),
  resizePane: (paneId: string, cols: number, rows: number) =>
    invoke<void>("resize_pane", { paneId, cols, rows }),
  setLayoutRatios: (terminalId: string, path: number[], ratios: number[]) =>
    invoke<void>("set_layout_ratios", { terminalId, path, ratios }),
  activeTerminal: () => invoke<string | null>("active_terminal"),
  setActiveTerminal: (terminalId: string) =>
    invoke<void>("set_active_terminal", { terminalId }),
  renameTerminal: (terminalId: string, label: string) =>
    invoke<TerminalSnapshot>("rename_terminal", { terminalId, label }),
  reorderTerminals: (ids: string[]) =>
    invoke<void>("reorder_terminals", { ids }),
  setAlwaysOnTop: (enabled: boolean) =>
    invoke<void>("set_always_on_top", { enabled }),
  extractPane: (paneId: string) =>
    invoke<TerminalSnapshot>("extract_pane", { paneId }),
  mergeTerminalIntoPane: (
    sourceTerminalId: string,
    targetPaneId: string,
    direction: SplitDir,
  ) =>
    invoke<TerminalSnapshot>("merge_terminal_into_pane", {
      sourceTerminalId,
      targetPaneId,
      direction,
    }),
  flipParentSplit: (paneId: string) =>
    invoke<TerminalSnapshot>("flip_parent_split", { paneId }),
  writeInputBroadcast: (terminalId: string, data: string) =>
    invoke<void>("write_input_broadcast", { terminalId, data }),
  getPlatform: () => invoke<string>("get_platform"),
  quitApp: () => invoke<void>("quit_app"),
  ackWoke: (generation: number) => invoke<void>("ack_woke", { generation }),
  moveTerminal: (terminalId: string, parentId: string | null, order: string[]) =>
    invoke<void>("move_terminal", { terminalId, parentId, order }),
  setTerminalUnread: (terminalId: string, unread: boolean) =>
    invoke<void>("set_terminal_unread", { terminalId, unread }),
  getPaneBacklog: (paneId: string) =>
    invoke<{ data: string; end: number }>("get_pane_backlog", { paneId }),
  getTerminalFiles: (terminalId: string) =>
    invoke<SessionFiles[]>("get_terminal_files", { terminalId }),
  stageTerminalFiles: (terminalId: string) =>
    invoke<StageResult>("stage_terminal_files", { terminalId }),
  clearTerminalFiles: (terminalId: string) =>
    invoke<void>("clear_terminal_files", { terminalId }),
  listClaudeSessions: () => invoke<ClaudeSession[]>("list_claude_sessions"),
  readBackgroundOutput: (paneId: string, taskId: string) =>
    invoke<BackgroundOutput>("read_background_output", { paneId, taskId }),
  fileDiff: (path: string) => invoke<string>("file_diff", { path }),
  openPath: (paneId: string, path: string, line?: number, col?: number) =>
    invoke<void>("open_path", { paneId, path, line: line ?? null, col: col ?? null }),
  setBadgeCount: (count: number) => invoke<void>("set_badge_count", { count }),
  openPortUrl: (terminalId: string, url: string) =>
    invoke<void>("open_port_url", { terminalId, url }),
  openDevcontainerShell: (paneId: string) =>
    invoke<TerminalSnapshot>("open_devcontainer_shell", { paneId }),
  forkClaudeSession: (paneId: string) =>
    invoke<string>("fork_claude_session", { paneId }),
  activeCwd: () => invoke<string | null>("active_cwd"),
  claudeIntegrationStatus: () =>
    invoke<IntegrationStatus>("claude_integration_status"),
  installClaudePlugin: () => invoke<string>("install_claude_plugin"),
  redrawTerminals: () => invoke<void>("redraw_terminals"),
};

// `offset` is the stream position of the chunk's first byte, used to drop
// output already replayed from the backlog.
export function onPtyOutput(
  paneId: string,
  handler: (chunk: Uint8Array, offset: number) => void,
): Promise<UnlistenFn> {
  return listen<{ o: number; d: string }>(`pty:output:${paneId}`, (e) => {
    handler(base64ToBytes(e.payload.d), e.payload.o);
  });
}

export function onPtyExit(
  paneId: string,
  handler: () => void,
): Promise<UnlistenFn> {
  return listen<null>(`pty:exit:${paneId}`, () => handler());
}

export function onTerminalAdded(
  handler: (t: TerminalSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<TerminalSnapshot>("terminal:added", (e) => handler(e.payload));
}

export function onTerminalRemoved(
  handler: (id: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("terminal:removed", (e) => handler(e.payload));
}

export function onTerminalUpdated(
  handler: (t: TerminalSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<TerminalSnapshot>("terminal:updated", (e) =>
    handler(e.payload),
  );
}

export function onPaneForeground(
  handler: (c: PaneForegroundChanged) => void,
): Promise<UnlistenFn> {
  return listen<PaneForegroundChanged>("pane:foreground", (e) =>
    handler(e.payload),
  );
}

export function onTerminalsReordered(
  handler: (ids: string[]) => void,
): Promise<UnlistenFn> {
  return listen<string[]>("terminals:reordered", (e) => handler(e.payload));
}

export function onPaneStats(
  handler: (stats: PaneStat[]) => void,
): Promise<UnlistenFn> {
  return listen<PaneStat[]>("panes:stats", (e) => handler(e.payload));
}

export function onAppCloseRequested(
  handler: (running: string[]) => void,
): Promise<UnlistenFn> {
  return listen<string[]>("app:close-requested", (e) => handler(e.payload));
}

// Emitted by the backend on a macOS system wake. The payload is the wake
// generation to pass back to `ackWoke`. See `wake.rs`.
export function onSystemWoke(
  handler: (generation: number) => void,
): Promise<UnlistenFn> {
  return listen<number>("system:woke", (e) => handler(e.payload));
}

export function onPaneClaude(
  handler: (c: PaneClaudeChanged) => void,
): Promise<UnlistenFn> {
  return listen<PaneClaudeChanged>("pane:claude", (e) => handler(e.payload));
}

// The control socket (MCP focus_tab) asked to bring a tab to the front.
export function onTerminalFocusRequested(
  handler: (id: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("terminal:focus-requested", (e) => handler(e.payload));
}

export function onPaneContext(handler: (c: PaneContext) => void): Promise<UnlistenFn> {
  return listen<PaneContext>("pane:context", (e) => handler(e.payload));
}

// Help → Keyboard Shortcuts (native menu, ⌘?).
export function onShowShortcutsRequested(handler: () => void): Promise<UnlistenFn> {
  return listen<null>("help:shortcuts", () => handler());
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
