import { writable, type Readable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface CustomShortcut {
  id: string;
  name: string;
  cwd?: string | null;
  command?: string | null;
  shell?: string | null;
}

export interface Shortcuts {
  schemaVersion: number;
  items: CustomShortcut[];
}

const DEFAULT: Shortcuts = {
  schemaVersion: 1,
  items: [],
};

const internal = writable<Shortcuts>(DEFAULT);

export const shortcuts: Readable<Shortcuts> = {
  subscribe: internal.subscribe,
};

export async function loadShortcuts(): Promise<void> {
  try {
    const s = await invoke<Shortcuts>("get_shortcuts");
    internal.set(s);
  } catch (e) {
    console.error("loadShortcuts:", e);
  }
}

export async function saveShortcuts(s: Shortcuts): Promise<void> {
  try {
    const saved = await invoke<Shortcuts>("set_shortcuts", { shortcuts: s });
    internal.set(saved);
  } catch (e) {
    console.error("saveShortcuts:", e);
    throw e;
  }
}

export function installShortcutsListener(): Promise<UnlistenFn> {
  return listen<Shortcuts>("shortcuts:changed", (e) => internal.set(e.payload));
}
