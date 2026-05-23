import { writable } from "svelte/store";
import { api } from "./api";

export type Platform = "mac" | "linux" | "other";

export const platform = writable<Platform>("mac");

export async function initPlatform(): Promise<void> {
  try {
    const os = await api.getPlatform();
    if (os === "macos") platform.set("mac");
    else if (os === "linux") platform.set("linux");
    else platform.set("other");
  } catch (e) {
    console.error("getPlatform failed:", e);
  }
}

export function shortcut(p: Platform, mac: string, linux: string): string {
  return p === "mac" ? mac : linux;
}
