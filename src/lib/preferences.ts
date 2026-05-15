import { writable, type Readable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface Preferences {
  schemaVersion: number;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  cursorBlink: boolean;
}

const DEFAULT: Preferences = {
  schemaVersion: 1,
  fontFamily:
    'Menlo, Monaco, "JetBrains Mono", "Fira Code", Consolas, monospace',
  fontSize: 13,
  lineHeight: 1.15,
  cursorBlink: true,
};

const internal = writable<Preferences>(DEFAULT);

export const preferences: Readable<Preferences> = {
  subscribe: internal.subscribe,
};

export async function loadPreferences(): Promise<void> {
  try {
    const p = await invoke<Preferences>("get_preferences");
    internal.set(p);
  } catch (e) {
    console.error("loadPreferences:", e);
  }
}

export async function savePreferences(p: Preferences): Promise<void> {
  try {
    const saved = await invoke<Preferences>("set_preferences", { prefs: p });
    internal.set(saved);
  } catch (e) {
    console.error("savePreferences:", e);
    throw e;
  }
}

export function installPreferencesListener() {
  listen<Preferences>("preferences:changed", (e) => internal.set(e.payload));
}
