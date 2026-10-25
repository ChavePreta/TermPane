import { writable } from "svelte/store";

/**
 * Requests from deep components (a pane's ⋯ menu) for UI owned by the page:
 * dialogs and the close confirmation. `+page.svelte` handles each one and
 * resets the store to null.
 */
export type UiRequest =
  | { kind: "files"; terminalId: string; autoStage: boolean }
  | { kind: "close"; terminalId: string; paneId: string };

export const uiRequest = writable<UiRequest | null>(null);
