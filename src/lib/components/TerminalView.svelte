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
  import { get } from "svelte/store";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  type Props = {
    paneId: string;
    active: boolean;
    terminalActive: boolean;
    onFocus?: () => void;
  };

  let { paneId, active, terminalActive, onFocus }: Props = $props();

  let container: HTMLDivElement;
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let resizeObs: ResizeObserver | null = null;
  let unlistenOutput: UnlistenFn | null = null;
  let unlistenExit: UnlistenFn | null = null;
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

    try {
      const webgl = new WebglAddon();
      term.loadAddon(webgl);
    } catch (e) {
      console.warn("WebGL addon falhou, usando canvas:", e);
    }

    term.onData((data) => {
      api.writeInput(paneId, data).catch((err) =>
        console.error("writeInput erro:", err),
      );
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
  }

  .xterm-host.active {
    border-color: var(--accent);
  }

  :global(.xterm) {
    height: 100%;
  }

  :global(.xterm-viewport) {
    background-color: transparent !important;
  }
</style>
