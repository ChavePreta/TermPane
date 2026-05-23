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

    try {
      term.loadAddon(new WebglAddon());
    } catch (e) {
      console.warn("WebGL addon failed:", e);
    }

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

    unlistenOutput = await onPtyOutput(paneId, (chunk) => {
      term?.write(chunk);
    });

    unlistenExit = await onPtyExit(paneId, () => {
      term?.write("\r\n\x1b[2;37m[process exited]\x1b[0m\r\n");
    });

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
    term?.dispose();
    term = null;
  });

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
  }
</style>
