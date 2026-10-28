import { writable } from "svelte/store";

/**
 * Progress a program reports with ConEmu's `OSC 9;4;<state>;<percent>` (the
 * sequence Windows Terminal, Ghostty and WezTerm also show), per pane:
 * 0 clears it, 1 sets the percent, 2 is an error, 3 has no percent
 * (indeterminate), 4 is paused/warning. 2 and 4 keep the last percent when
 * they carry none.
 *
 * Ephemeral, and cleared when the pane is back at its shell prompt, so a
 * script that dies without clearing it leaves no bar behind.
 */
export type ProgressState = "normal" | "error" | "indeterminate" | "paused";
export type PaneProgress = { terminalId: string; state: ProgressState; percent: number };

const progress = writable<Map<string, PaneProgress>>(new Map());

export const paneProgress = { subscribe: progress.subscribe };

const STATES: Record<string, ProgressState> = {
  "1": "normal",
  "2": "error",
  "3": "indeterminate",
  "4": "paused",
};

/** Applies the payload of one `OSC 9;4` sequence (`4;<state>[;<percent>]`). */
export function applyProgress(terminalId: string, paneId: string, data: string) {
  const [, st, pr] = data.split(";");
  const state = STATES[st ?? ""];
  if (!state) {
    clearProgress(paneId);
    return;
  }
  const value = Number.parseInt(pr ?? "", 10);
  progress.update((m) => {
    const prev = m.get(paneId);
    const percent = Number.isFinite(value)
      ? Math.min(100, Math.max(0, value))
      : (prev?.percent ?? 0);
    const next = new Map(m);
    next.set(paneId, { terminalId, state, percent });
    return next;
  });
}

export function clearProgress(paneId: string) {
  progress.update((m) => {
    if (!m.has(paneId)) return m;
    const next = new Map(m);
    next.delete(paneId);
    return next;
  });
}

/** The progress a card shows: an error first, else the furthest along. */
export function terminalProgress(
  m: Map<string, PaneProgress>,
  terminalId: string,
): PaneProgress | null {
  const rank = (p: PaneProgress) => (p.state === "error" ? 1000 : 0) + p.percent;
  let best: PaneProgress | null = null;
  for (const p of m.values()) {
    if (p.terminalId === terminalId && (!best || rank(p) > rank(best))) best = p;
  }
  return best;
}
