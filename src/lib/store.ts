import { writable, derived, get } from "svelte/store";
import {
  api,
  type ClaudeInfo,
  type ClaudeStatus,
  type GitInfo,
  type PaneContext,
  type PortInfo,
  type PaneStat,
  type TerminalSnapshot,
} from "./api";

function createTerminalsStore() {
  const { subscribe, update, set } = writable<TerminalSnapshot[]>([]);

  return {
    subscribe,
    set,
    add: (t: TerminalSnapshot) =>
      update((list) => {
        if (list.some((x) => x.id === t.id)) return list;
        return [...list, t];
      }),
    remove: (id: string) => update((list) => list.filter((t) => t.id !== id)),
    upsert: (t: TerminalSnapshot) =>
      update((list) => {
        const i = list.findIndex((x) => x.id === t.id);
        if (i === -1) return [...list, t];
        const next = [...list];
        next[i] = t;
        return next;
      }),
    setPaneForeground: (
      terminalId: string,
      paneId: string,
      command: string | null,
    ) =>
      update((list) =>
        list.map((t) => {
          if (t.id !== terminalId) return t;
          return {
            ...t,
            panes: t.panes.map((p) =>
              p.id === paneId ? { ...p, foregroundCommand: command } : p,
            ),
          };
        }),
      ),
    setPaneClaude: (
      terminalId: string,
      paneId: string,
      claude: ClaudeInfo | null,
    ) =>
      update((list) =>
        list.map((t) => {
          if (t.id !== terminalId) return t;
          return {
            ...t,
            panes: t.panes.map((p) => (p.id === paneId ? { ...p, claude } : p)),
          };
        }),
      ),
    setPaneContext: (c: PaneContext) =>
      update((list) =>
        list.map((t) =>
          t.id !== c.terminalId
            ? t
            : {
                ...t,
                panes: t.panes.map((p) =>
                  p.id === c.paneId
                    ? { ...p, git: c.git, ports: c.ports, devContainer: c.devContainer, cwd: c.cwd }
                    : p,
                ),
              },
        ),
      ),
    setPaneStats: (stats: PaneStat[]) => {
      if (stats.length === 0) return;
      const byTerminal = new Map<string, Map<string, number>>();
      for (const s of stats) {
        let inner = byTerminal.get(s.terminalId);
        if (!inner) {
          inner = new Map();
          byTerminal.set(s.terminalId, inner);
        }
        inner.set(s.paneId, s.memoryBytes);
      }
      update((list) =>
        list.map((t) => {
          const inner = byTerminal.get(t.id);
          if (!inner) return t;
          return {
            ...t,
            panes: t.panes.map((p) => {
              const m = inner.get(p.id);
              return m === undefined ? p : { ...p, memoryBytes: m };
            }),
          };
        }),
      );
    },
    reorder: (ids: string[]) =>
      update((list) => {
        const byId = new Map(list.map((t) => [t.id, t]));
        const next = ids
          .map((id) => byId.get(id))
          .filter((t): t is TerminalSnapshot => !!t);
        // Append any terminals missing from the payload at the end (defensive).
        for (const t of list) {
          if (!ids.includes(t.id)) next.push(t);
        }
        return next;
      }),
  };
}

export const terminals = createTerminalsStore();
export const activeId = writable<string | null>(null);

/**
 * Switch the active terminal. Sets `activeId` optimistically for instant
 * feedback, then syncs the backend, reverting on failure so the frontend's
 * active card and the backend's `AppState.active` can't diverge (a stale
 * backend active would make the next `Cmd+T` inherit the wrong cwd). Shared by
 * sidebar clicks and the keyboard switch-terminal shortcut.
 */
export async function selectTerminal(id: string): Promise<void> {
  const prev = get(activeId);
  activeId.set(id);
  try {
    await api.setActiveTerminal(id);
  } catch (e) {
    console.error("setActiveTerminal:", e);
    if (get(activeId) === id) activeId.set(prev);
  }
}

export const activeTerminal = derived(
  [terminals, activeId],
  ([$terminals, $activeId]) =>
    $terminals.find((t) => t.id === $activeId) ?? null,
);

export function activePaneForeground(
  terminal: TerminalSnapshot,
): string | null {
  const p = terminal.panes.find((x) => x.id === terminal.activePane);
  return p?.foregroundCommand ?? null;
}

/**
 * Joins the foreground commands of every pane in a terminal.
 * Order follows the Leaf order in the layout (visual top-left → bottom-right).
 * Truncates the result with "…" if it gets too long.
 */
export function aggregateForeground(
  terminal: TerminalSnapshot,
  maxLen = 60,
): string | null {
  const order = layoutLeafOrder(terminal.layout);
  const cmds = order
    .map((id) => terminal.panes.find((p) => p.id === id)?.foregroundCommand)
    .filter((c): c is string => !!c);
  if (cmds.length === 0) return null;
  const joined = cmds.join(" / ");
  if (joined.length <= maxLen) return joined;
  return joined.slice(0, maxLen - 1) + "…";
}

function layoutLeafOrder(
  node: TerminalSnapshot["layout"],
  acc: string[] = [],
): string[] {
  if (node.type === "leaf") {
    acc.push(node.id);
  } else {
    for (const c of node.children) layoutLeafOrder(c, acc);
  }
  return acc;
}

// Most urgent first: what the card shows when several panes run Claude.
const CLAUDE_PRIORITY: ClaudeStatus[] = [
  "permission",
  "waiting",
  "tool",
  "thinking",
  "done",
  "idle",
  "ended",
];

/** The Claude session in this terminal that most needs to be shown. */
export function terminalClaude(terminal: TerminalSnapshot): ClaudeInfo | null {
  let best: ClaudeInfo | null = null;
  for (const p of terminal.panes) {
    const c = p.claude;
    if (!c) continue;
    if (
      !best ||
      CLAUDE_PRIORITY.indexOf(c.status) < CLAUDE_PRIORITY.indexOf(best.status)
    ) {
      best = c;
    }
  }
  return best;
}

export function terminalFileCount(terminal: TerminalSnapshot): number {
  const all = new Set<string>();
  for (const p of terminal.panes) for (const f of p.claude?.files ?? []) all.add(f);
  return all.size;
}

/** Git context of the terminal's active pane, else of any pane that has one. */
export function terminalGit(terminal: TerminalSnapshot): GitInfo | null {
  const active = terminal.panes.find((p) => p.id === terminal.activePane);
  return active?.git ?? terminal.panes.find((p) => p.git)?.git ?? null;
}

/** Servers of all the terminal's panes, one per address. */
export function terminalPorts(terminal: TerminalSnapshot): PortInfo[] {
  const seen = new Map<string, PortInfo>();
  for (const p of terminal.panes) for (const port of p.ports ?? []) seen.set(port.address, port);
  return [...seen.values()];
}

export function needsYou(terminal: TerminalSnapshot): boolean {
  return terminal.panes.some(
    (p) => p.claude?.status === "permission" || p.claude?.status === "waiting",
  );
}

/** Nesting depth of each terminal (0 = top level). */
export function terminalDepths(list: TerminalSnapshot[]): Map<string, number> {
  const byId = new Map(list.map((t) => [t.id, t]));
  const depths = new Map<string, number>();
  for (const t of list) {
    let d = 0;
    let cur = t.parentId ? byId.get(t.parentId) : undefined;
    while (cur && d < list.length) {
      d++;
      cur = cur.parentId ? byId.get(cur.parentId) : undefined;
    }
    depths.set(t.id, d);
  }
  return depths;
}

/** `id` plus every descendant, in list (depth-first) order. */
export function subtreeIds(list: TerminalSnapshot[], id: string): string[] {
  const out = [id];
  const inTree = new Set(out);
  for (const t of list) {
    if (t.parentId && inTree.has(t.parentId) && !inTree.has(t.id)) {
      out.push(t.id);
      inTree.add(t.id);
    }
  }
  return out;
}

export function aggregateMemory(terminal: TerminalSnapshot): number {
  let total = 0;
  for (const p of terminal.panes) {
    if (typeof p.memoryBytes === "number") total += p.memoryBytes;
  }
  return total;
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "";
  const KB = 1024;
  const MB = KB * 1024;
  const GB = MB * 1024;
  if (n >= GB) return (n / GB).toFixed(2) + " GB";
  if (n >= MB) {
    const v = n / MB;
    return (v >= 100 ? v.toFixed(0) : v.toFixed(1)) + " MB";
  }
  if (n >= KB) return Math.round(n / KB) + " KB";
  return n + " B";
}
