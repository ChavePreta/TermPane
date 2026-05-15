/**
 * Per-pane action registry that the global keydown handler invokes
 * (clear, scroll-to-bottom, etc.). Non-reactive: plain Map.
 */

export interface PaneActions {
  clear(): void;
}

const actions = new Map<string, PaneActions>();

export function registerPaneActions(paneId: string, a: PaneActions): void {
  actions.set(paneId, a);
}

export function unregisterPaneActions(paneId: string): void {
  actions.delete(paneId);
}

export function getPaneActions(paneId: string): PaneActions | undefined {
  return actions.get(paneId);
}
