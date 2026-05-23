import { writable } from "svelte/store";

/**
 * Pane id currently highlighted as the drop target for a sidebar drag.
 * `null` when no drag is in progress or the pointer is back over the sidebar.
 */
export const dropTargetPaneId = writable<string | null>(null);
