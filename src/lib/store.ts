import { writable, derived } from "svelte/store";
import type { PaneStat, TerminalSnapshot } from "./api";

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
