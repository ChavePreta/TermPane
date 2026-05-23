import { writable, get } from "svelte/store";

/** Terminal ids that currently have input broadcast enabled. */
export const broadcastEnabled = writable<Set<string>>(new Set());

export function toggleBroadcast(terminalId: string): void {
  broadcastEnabled.update((set) => {
    const next = new Set(set);
    if (next.has(terminalId)) next.delete(terminalId);
    else next.add(terminalId);
    return next;
  });
}

export function isBroadcasting(terminalId: string): boolean {
  return get(broadcastEnabled).has(terminalId);
}

export function disableBroadcast(terminalId: string): void {
  broadcastEnabled.update((set) => {
    if (!set.has(terminalId)) return set;
    const next = new Set(set);
    next.delete(terminalId);
    return next;
  });
}
