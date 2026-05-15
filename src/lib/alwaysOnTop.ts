import { writable } from "svelte/store";
import { browser } from "$app/environment";
import { api } from "./api";

const STORAGE_KEY = "termpane.alwaysOnTop";

function initial(): boolean {
  if (!browser) return false;
  return localStorage.getItem(STORAGE_KEY) === "1";
}

export const alwaysOnTop = writable<boolean>(initial());

if (browser) {
  alwaysOnTop.subscribe((v) => {
    try {
      localStorage.setItem(STORAGE_KEY, v ? "1" : "0");
    } catch {}
    api.setAlwaysOnTop(v).catch((e) =>
      console.error("setAlwaysOnTop falhou:", e),
    );
  });
}
