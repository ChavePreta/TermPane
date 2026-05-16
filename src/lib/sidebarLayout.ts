import { writable } from "svelte/store";
import { browser } from "$app/environment";

const COLLAPSED_KEY = "termpane.sidebar.collapsed";
const WIDTH_KEY = "termpane.sidebar.width";

export const MIN_WIDTH = 180;
export const MAX_WIDTH = 500;
export const DEFAULT_WIDTH = 240;

function clamp(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_WIDTH;
  return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, Math.round(n)));
}

function initialCollapsed(): boolean {
  if (!browser) return false;
  return localStorage.getItem(COLLAPSED_KEY) === "1";
}

function initialWidth(): number {
  if (!browser) return DEFAULT_WIDTH;
  const raw = localStorage.getItem(WIDTH_KEY);
  if (!raw) return DEFAULT_WIDTH;
  const n = parseInt(raw, 10);
  return Number.isFinite(n) ? clamp(n) : DEFAULT_WIDTH;
}

export const sidebarCollapsed = writable<boolean>(initialCollapsed());

const widthInternal = writable<number>(initialWidth());
export const sidebarWidth = {
  subscribe: widthInternal.subscribe,
  set: (v: number) => widthInternal.set(clamp(v)),
  update: (fn: (v: number) => number) =>
    widthInternal.update((v) => clamp(fn(v))),
};

if (browser) {
  sidebarCollapsed.subscribe((v) => {
    try {
      localStorage.setItem(COLLAPSED_KEY, v ? "1" : "0");
    } catch {}
  });
  widthInternal.subscribe((v) => {
    try {
      localStorage.setItem(WIDTH_KEY, String(v));
    } catch {}
  });
}
