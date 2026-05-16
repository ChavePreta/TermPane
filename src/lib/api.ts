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

export interface PaneSnapshot {
  id: string;
  foregroundCommand: string | null;
  exitCode: number | null;
  memoryBytes?: number;
}

export interface TerminalSnapshot {
  id: string;
  label: string;
  createdAt: string;
  layout: LayoutSnapshot;
  panes: PaneSnapshot[];
  activePane: string;
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
  openTerminal: () => invoke<TerminalSnapshot>("open_terminal"),
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
  quitApp: () => invoke<void>("quit_app"),
};

export function onPtyOutput(
  paneId: string,
  handler: (chunk: Uint8Array) => void,
): Promise<UnlistenFn> {
  return listen<string>(`pty:output:${paneId}`, (e) => {
    handler(base64ToBytes(e.payload));
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

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
