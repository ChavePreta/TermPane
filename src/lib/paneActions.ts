/**
 * Per-pane action registry that the global keydown handler invokes
 * (clear, scroll-to-bottom, etc.). Non-reactive: plain Map.
 */

export interface PaneActions {
  clear(): void;
  // Refit and repaint. `hard` also recreates the WebGL context (after a short
  // DOM-rendered grace period) — for system wake, where WKWebView can leave a
  // pane blank, white or black.
  recover(hard: boolean): void;
  // Scroll to the previous (-1) / next (1) prompt typed to Claude Code.
  jumpToPrompt(dir: -1 | 1): void;
  // Scroll to the start / end of the scrollback.
  scrollToEdge(edge: "top" | "bottom"): void;
  // Scrollback search (⌘F): open the bar, or step to the next / previous match.
  openSearch(): void;
  searchStep(dir: -1 | 1): void;
  // The popover listing every prompt typed to Claude in this pane.
  togglePromptList(): void;
  // Copy the selection exactly as displayed, without joining wrapped rows.
  copyRaw(): void;
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

// Recover every mounted pane (all terminals stay mounted, only the active one
// is visible). One throwing pane must not abort the sweep.
export function recoverAllPanes(hard: boolean): void {
  for (const a of actions.values()) {
    try {
      a.recover(hard);
    } catch (e) {
      console.error("pane recover failed:", e);
    }
  }
}
