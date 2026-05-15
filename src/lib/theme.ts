import { writable, derived, type Readable } from "svelte/store";
import { browser } from "$app/environment";
import type { ITheme } from "@xterm/xterm";

export type ThemeChoice = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "termpane.theme";

function readInitial(): ThemeChoice {
  if (!browser) return "system";
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

export const themeChoice = writable<ThemeChoice>(readInitial());

const systemPrefersDark = writable<boolean>(
  browser
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : true,
);

if (browser) {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", (e) => systemPrefersDark.set(e.matches));

  themeChoice.subscribe((c) => {
    try {
      localStorage.setItem(STORAGE_KEY, c);
    } catch {}
  });
}

export const resolvedTheme: Readable<ResolvedTheme> = derived(
  [themeChoice, systemPrefersDark],
  ([$choice, $sysDark]) => {
    if ($choice === "system") return $sysDark ? "dark" : "light";
    return $choice;
  },
);

if (browser) {
  resolvedTheme.subscribe((t) => {
    document.documentElement.setAttribute("data-theme", t);
  });
}

export const xtermThemes: Record<ResolvedTheme, ITheme> = {
  dark: {
    background: "#1a1b26",
    foreground: "#c0caf5",
    cursor: "#7aa2f7",
    cursorAccent: "#1a1b26",
    selectionBackground: "#364a82",
    black: "#15161e",
    red: "#f7768e",
    green: "#9ece6a",
    yellow: "#e0af68",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#a9b1d6",
    brightBlack: "#414868",
    brightRed: "#f7768e",
    brightGreen: "#9ece6a",
    brightYellow: "#e0af68",
    brightBlue: "#7aa2f7",
    brightMagenta: "#bb9af7",
    brightCyan: "#7dcfff",
    brightWhite: "#c0caf5",
  },
  light: {
    background: "#ffffff",
    foreground: "#1c1d22",
    cursor: "#4d6cff",
    cursorAccent: "#ffffff",
    selectionBackground: "#cfdcff",
    black: "#1c1d22",
    red: "#c83040",
    green: "#1f8a4d",
    yellow: "#b87b00",
    blue: "#4d6cff",
    magenta: "#bb35cc",
    cyan: "#007aa5",
    white: "#6e6e73",
    brightBlack: "#8e8e93",
    brightRed: "#e0455a",
    brightGreen: "#2ea863",
    brightYellow: "#cf8e0f",
    brightBlue: "#6b86ff",
    brightMagenta: "#cb55da",
    brightCyan: "#1095c1",
    brightWhite: "#1c1d22",
  },
};
