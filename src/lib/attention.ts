import { writable, get } from "svelte/store";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { terminals, activeId } from "./store";
import { preferences } from "./preferences";

/**
 * Per-terminal bell state. `nonce` bumps on every bell so the card can restart
 * its pulse animation (immediate "ding" feedback, shown for *any* terminal).
 * `persistent` is set only when the bell rang on a terminal the user wasn't
 * watching — it drives the lingering dot + highlight that survives until the
 * terminal is viewed.
 *
 * Ephemeral (like broadcast state) — never persisted.
 */
export type AttentionEntry = { nonce: number; persistent: boolean };

const attention = writable<Map<string, AttentionEntry>>(new Map());

export const attentionTerminals = { subscribe: attention.subscribe };

export function clearAttention(terminalId: string) {
  notified.delete(terminalId);
  attention.update((m) => {
    if (!m.has(terminalId)) return m;
    const next = new Map(m);
    next.delete(terminalId);
    return next;
  });
}

function pushBell(terminalId: string, persistent: boolean) {
  attention.update((m) => {
    const prev = m.get(terminalId);
    const next = new Map(m);
    next.set(terminalId, {
      nonce: (prev?.nonce ?? 0) + 1,
      persistent: persistent || (prev?.persistent ?? false),
    });
    return next;
  });
}

// Permission is requested once on the first background bell. We cache only the
// *granted* result; while not granted we re-check the OS on each call so that
// enabling notifications later in System Settings recovers without an app
// restart. The permission prompt is only ever shown once.
let permissionGranted = false;
let permissionRequested = false;

async function ensurePermission(): Promise<boolean> {
  if (permissionGranted) return true;
  try {
    let granted = await isPermissionGranted();
    if (!granted && !permissionRequested) {
      permissionRequested = true;
      granted = (await requestPermission()) === "granted";
    }
    permissionGranted = granted;
    return granted;
  } catch (e) {
    console.error("notification permission:", e);
    return false;
  }
}

// Terminals we've already fired an OS notification for, cleared once the user
// views the terminal (clearAttention). Distinct from the persistent flag: that
// can be set by a bell received while the window was focused (no notification
// sent), and keying the throttle on it would suppress the later, genuinely
// needed background notification.
const notified = new Set<string>();

async function notify(terminalId: string, message: string | null) {
  if (!(await ensurePermission())) return;
  const label =
    get(terminals).find((t) => t.id === terminalId)?.label ?? "Terminal";
  const body = message ? `${label}: ${message}` : `${label} needs your attention`;
  try {
    sendNotification({ title: "TermPane", body });
  } catch (e) {
    console.error("sendNotification:", e);
  }
}

/**
 * Flag `terminalId` as needing attention. The card always pulses (immediate
 * feedback). If the user isn't currently watching that terminal — it's not the
 * active card, or the window is unfocused — the card additionally keeps a
 * persistent dot + highlight until viewed, and when the window is in the
 * background a native OS notification fires (unless disabled in preferences).
 *
 * The `notified` set throttles a chatty program to one OS notification per
 * terminal until the user actually visits it.
 */
function flag(terminalId: string, message: string | null) {
  const isActive = get(activeId) === terminalId;
  const focused = document.hasFocus();
  const watching = isActive && focused;

  pushBell(terminalId, !watching);

  // A native notification only makes sense when the window is in the background
  // (which already implies the user isn't watching this terminal).
  if (!focused && !notified.has(terminalId) && get(preferences).bellNotifications) {
    notified.add(terminalId);
    void notify(terminalId, message);
  }
}

/** The terminal rang the bell (`\x07`) — e.g. Claude finished a turn. */
export function signalBell(terminalId: string) {
  flag(terminalId, null);
}

/**
 * A desktop-notification escape sequence (OSC 9 / OSC 777) was received — the
 * iTerm2/Ghostty/VS Code convention many CLI tools use. Same effect as a bell,
 * but the tool's message rides along to the OS notification.
 */
export function signalNotification(terminalId: string, message: string) {
  flag(terminalId, message.trim() || null);
}
