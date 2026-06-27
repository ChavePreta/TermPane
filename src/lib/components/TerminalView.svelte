<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { api, onPtyOutput, onPtyExit } from "../api";
  import { resolvedTheme, xtermThemes } from "../theme";
  import { preferences } from "../preferences";
  import { registerPaneActions, unregisterPaneActions } from "../paneActions";
  import { signalBell, signalNotification } from "../attention";
  import { broadcastEnabled } from "../broadcast";
  import { dropTargetPaneId } from "../dragState";
  import { get } from "svelte/store";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  type Props = {
    paneId: string;
    terminalId: string;
    active: boolean;
    terminalActive: boolean;
    onFocus?: () => void;
  };

  let { paneId, terminalId, active, terminalActive, onFocus }: Props = $props();

  let isDropTarget = $derived($dropTargetPaneId === paneId);

  let container: HTMLDivElement;
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let resizeObs: ResizeObserver | null = null;
  let unlistenOutput: UnlistenFn | null = null;
  let unlistenExit: UnlistenFn | null = null;
  let unlistenWheel: (() => void) | null = null;
  let unsubTheme: (() => void) | null = null;
  let unsubPrefs: (() => void) | null = null;
  let resizeTimer: ReturnType<typeof setTimeout> | null = null;
  let lastCols = 0;
  let lastRows = 0;
  let webglFailures = 0;
  let webglRetryTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(async () => {
    const prefs = get(preferences);
    term = new Terminal({
      cursorBlink: prefs.cursorBlink,
      fontFamily: prefs.fontFamily,
      fontSize: prefs.fontSize,
      lineHeight: prefs.lineHeight,
      allowProposedApi: true,
      scrollback: 10000,
      theme: xtermThemes[get(resolvedTheme)],
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(new WebLinksAddon());

    term.open(container);

    const onWheel = (e: WheelEvent) => {
      if (!term || e.deltaY === 0) return;
      // App opted into mouse tracking — let xterm forward the wheel as an
      // SGR mouse sequence so TUIs like vim/less still get their events.
      if (term.modes.mouseTrackingMode !== "none") return;
      e.preventDefault();
      const fontSize = term.options.fontSize ?? 13;
      const lineHeight = term.options.lineHeight ?? 1;
      const pxPerLine = Math.max(1, fontSize * lineHeight);
      const magnitude = Math.max(
        1,
        Math.round(Math.abs(e.deltaY) / pxPerLine),
      );
      term.scrollLines(magnitude * Math.sign(e.deltaY));
      const buf = term.buffer.active;
      if (e.deltaY > 0 && buf.baseY - buf.viewportY <= 1) {
        term.scrollToBottom();
      }
    };
    container.addEventListener("wheel", onWheel, { passive: false });
    unlistenWheel = () => container.removeEventListener("wheel", onWheel);

    loadWebgl();

    term.onData((data) => {
      if (get(broadcastEnabled).has(terminalId)) {
        api.writeInputBroadcast(terminalId, data).catch((err) =>
          console.error("writeInputBroadcast erro:", err),
        );
      } else {
        api.writeInput(paneId, data).catch((err) =>
          console.error("writeInput erro:", err),
        );
      }
    });

    // A program rang the bell (\x07) — e.g. Claude finished and is waiting for
    // input. Flag the owning card unless the user is already watching it.
    term.onBell(() => signalBell(terminalId));

    // Desktop-notification escape sequences (iTerm2 OSC 9, OSC 777) — the
    // convention many CLI tools (and Claude Code's iterm2 channel) use to ask
    // for a notification. Treat them like a bell, carrying the tool's message.
    term.parser.registerOscHandler(9, (data) => {
      // ConEmu repurposes OSC 9;4;<state>;<progress> for progress reporting — not
      // a notification. Match that grammar (a digit right after "4;") so we don't
      // also swallow an iTerm2 notification whose text merely begins with "4;".
      if (!/^4;\d/.test(data)) signalNotification(terminalId, data);
      return true;
    });
    term.parser.registerOscHandler(777, (data) => {
      const parts = data.split(";");
      if (parts[0] === "notify") {
        const title = parts[1] ?? "";
        const body = parts.slice(2).join(";");
        signalNotification(terminalId, body || title);
      }
      return true;
    });

    unlistenOutput = await onPtyOutput(paneId, (chunk) => {
      term?.write(chunk);
    });

    unlistenExit = await onPtyExit(paneId, () => {
      term?.write("\r\n\x1b[2;37m[process exited]\x1b[0m\r\n");
    });

    // Flush a one-shot startup command (custom-shortcut "command") now that the
    // output listener above is attached, so the command's echo and early output
    // aren't lost. Returns null for splits and plain terminals.
    try {
      const startup = await api.takeStartupCommand(paneId);
      if (startup) await api.writeInput(paneId, `${startup}\n`);
    } catch (err) {
      console.error("startup command:", err);
    }

    await tick();
    fitNow();

    resizeObs = new ResizeObserver(() => scheduleFit());
    resizeObs.observe(container);

    unsubTheme = resolvedTheme.subscribe((t) => {
      if (term) term.options.theme = xtermThemes[t];
    });

    registerPaneActions(paneId, {
      clear: () => term?.clear(),
    });

    let firstPrefs = true;
    unsubPrefs = preferences.subscribe((p) => {
      if (firstPrefs) {
        firstPrefs = false;
        return;
      }
      if (!term) return;
      term.options.fontFamily = p.fontFamily;
      term.options.fontSize = p.fontSize;
      term.options.lineHeight = p.lineHeight;
      term.options.cursorBlink = p.cursorBlink;
      // Font changes require a re-fit to recompute cols/rows.
      scheduleFit();
    });
  });

  onDestroy(() => {
    unregisterPaneActions(paneId);
    unlistenOutput?.();
    unlistenExit?.();
    unlistenWheel?.();
    unsubTheme?.();
    unsubPrefs?.();
    resizeObs?.disconnect();
    if (resizeTimer) clearTimeout(resizeTimer);
    if (webglRetryTimer) clearTimeout(webglRetryTimer);
    term?.dispose();
    term = null;
  });

  function loadWebgl() {
    // Bail after repeated consecutive failures — stay on the DOM renderer
    // instead of churning through addons forever. webglFailures resets on a
    // successful load, so this catches a tight failure loop, not the occasional
    // sleep/wake recovery.
    if (!term || webglFailures >= 4) return;
    let addon: WebglAddon | null = null;
    try {
      addon = new WebglAddon();
      addon.onContextLoss(() => {
        // The GPU context was torn down — most often when the machine wakes
        // from sleep. xterm waits ~3s for the browser to restore it before
        // firing this; once it does, drop the dead addon so xterm falls back
        // to the DOM renderer, refit, and retry WebGL after the GPU settles.
        addon?.dispose();
        webglFailures++;
        scheduleFit();
        scheduleWebglRetry();
      });
      term.loadAddon(addon);
      webglFailures = 0;
      // The first fit is owned by onMount's explicit fitNow(); only schedule one
      // here when re-loading after a context loss (geometry already established).
      if (lastCols !== 0 || lastRows !== 0) scheduleFit();
    } catch (e) {
      console.warn("WebGL addon unavailable, using DOM renderer:", e);
      addon?.dispose();
      webglFailures++;
      scheduleWebglRetry();
    }
  }

  function scheduleWebglRetry() {
    if (webglRetryTimer) clearTimeout(webglRetryTimer);
    webglRetryTimer = setTimeout(loadWebgl, 1000);
  }

  function scheduleFit() {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(fitNow, 16);
  }

  function fitNow() {
    if (!fitAddon || !term || !container.isConnected) return;
    if (container.clientWidth === 0 || container.clientHeight === 0) return;
    try {
      fitAddon.fit();
      const cols = term.cols;
      const rows = term.rows;
      if (cols !== lastCols || rows !== lastRows) {
        lastCols = cols;
        lastRows = rows;
        api.resizePane(paneId, cols, rows).catch((err) =>
          console.error("resizePane erro:", err),
        );
      }
    } catch (e) {
      console.warn("fit falhou:", e);
    }
  }

  $effect(() => {
    if (active && terminalActive && term) {
      tick().then(() => {
        fitNow();
        term?.focus();
      });
    }
  });

  function handleHostClick() {
    onFocus?.();
    term?.focus();
  }
</script>

<div
  bind:this={container}
  class="xterm-host"
  class:active
  class:is-drop-target={isDropTarget}
  data-pane-id={paneId}
  onclick={handleHostClick}
  onfocusin={handleHostClick}
  role="presentation"
></div>

<style>
  .xterm-host {
    width: 100%;
    height: 100%;
    padding: 8px;
    background: var(--bg);
    box-sizing: border-box;
    border: 1px solid transparent;
    border-radius: 4px;
    transition: border-color 0.12s;
    position: relative;
  }

  .xterm-host.active {
    border-color: var(--accent);
  }

  .xterm-host.is-drop-target::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    border: 2px solid var(--accent);
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  :global(.xterm) {
    height: 100%;
  }

  :global(.xterm-viewport) {
    background-color: transparent !important;
    /* The WebGL canvas (.xterm-screen) is a later sibling and paints over the
       viewport's scrollbar, hiding it. Lift the viewport above the canvas so
       the custom scrollbar is visible (selection and drag still work). */
    z-index: 10;
  }

  :global(.xterm-viewport)::-webkit-scrollbar {
    /* macOS WKWebView keeps the native overlay scrollbar (ignoring width and
       the custom thumb/track) unless the native appearance is turned off. */
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
  }

  :global(.xterm-viewport)::-webkit-scrollbar-track {
    background: transparent;
  }

  :global(.xterm-viewport)::-webkit-scrollbar-thumb {
    /* Derive the thumb from the foreground color so it keeps usable contrast
       in both themes — var(--fg-faint) on the dark terminal background is the
       same hue as "bright black" and reads as nearly invisible. */
    background: color-mix(in srgb, var(--fg) 35%, transparent);
    border: 3px solid var(--bg);
    border-radius: 6px;
    background-clip: padding-box;
    min-height: 24px;
  }

  :global(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
    background: color-mix(in srgb, var(--fg) 55%, transparent);
    background-clip: padding-box;
  }
</style>
