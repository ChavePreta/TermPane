<script lang="ts" module>
  // WebKit caps live WebGL contexts per page (~16) and silently kills the
  // oldest one past that, blanking its canvas until xterm gives up on it ~3s
  // later. Only panes of the visible terminal hold a context, and never more
  // than this many at once; the rest use the DOM renderer.
  const MAX_WEBGL_CONTEXTS = 8;
  let liveWebglContexts = 0;
</script>

<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { SearchAddon } from "@xterm/addon-search";
  import { api, onPtyOutput, onPtyExit, base64ToBytes } from "../api";
  import { resolvedTheme, xtermThemes } from "../theme";
  import { preferences } from "../preferences";
  import { registerPaneActions, unregisterPaneActions } from "../paneActions";
  import { signalBell, signalNotification } from "../attention";
  import { broadcastEnabled } from "../broadcast";
  import { dropTargetPaneId } from "../dragState";
  import { terminals, selectTerminal } from "../store";
  import { platform, shortcut } from "../platform";
  import { uiRequest } from "../uiRequests";
  import TabContextMenu, { type MenuItem } from "./TabContextMenu.svelte";
  import BackgroundDialog from "./BackgroundDialog.svelte";
  import { openExternal } from "../api";
  import { joinRows, wrapWidth, type CopyRow } from "../copyText";
  import { copyToClipboard } from "../clipboard";
  import type { CodeBlock } from "../api";
  import type { IDecoration, IMarker } from "@xterm/xterm";
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

  let terminal = $derived($terminals.find((t) => t.id === terminalId));
  let pane = $derived(terminal?.panes.find((p) => p.id === paneId));

  // Prompt navigation arrows show while Claude Code runs in this pane.
  let claudeHere = $derived.by(() => {
    if (!pane) return false;
    if (pane.claude && pane.claude.status !== "ended") return true;
    return /(^|\/)claude(\s|$)/.test(pane.foregroundCommand ?? "");
  });

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
  let webglAddon: WebglAddon | null = null;
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
    searchAddon = new SearchAddon();
    term.loadAddon(searchAddon);
    searchAddon.onDidChangeResults(({ resultIndex, resultCount }) => {
      searchResult = { index: resultIndex, count: resultCount };
    });
    registerPathLinks(term);
    container.addEventListener("copy", handleCopy, true);

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

    if (terminalActive) loadWebgl();

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

    // The TUI may redraw (resume, recap, clear) after a copy button was
    // placed: check the buttons again once output settles.
    term.onWriteParsed(() => {
      if (codeBlocks.length > 0) schedulePlacement(codeBlocks, [700]);
    });

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

    // Replay the pane's backlog (its output so far — everything, for a view
    // remounted after a webview reload), then live chunks. Chunks that arrive
    // while the backlog is in flight are queued, and any part the backlog
    // already covered is dropped by stream offset.
    let replayedUpTo = -1;
    const pending: [Uint8Array, number][] = [];
    const writeChunk = (chunk: Uint8Array, offset: number) => {
      const end = offset + chunk.length;
      if (end <= replayedUpTo) return;
      term?.write(offset < replayedUpTo ? chunk.subarray(replayedUpTo - offset) : chunk);
    };
    unlistenOutput = await onPtyOutput(paneId, (chunk, offset) => {
      if (replayedUpTo < 0) pending.push([chunk, offset]);
      else writeChunk(chunk, offset);
    });
    try {
      const backlog = await api.getPaneBacklog(paneId);
      term?.write(base64ToBytes(backlog.data));
      replayedUpTo = backlog.end;
    } catch (err) {
      console.error("getPaneBacklog:", err);
      replayedUpTo = 0;
    }
    for (const [chunk, offset] of pending.splice(0)) writeChunk(chunk, offset);

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
      recover,
      jumpToPrompt,
      scrollToEdge,
      openSearch,
      searchStep: (dir) => runSearch(dir),
      togglePromptList: () => (promptListOpen ? (promptListOpen = false) : openPromptList()),
      copyRaw,
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
    clearCodeButtons();
    container?.removeEventListener("copy", handleCopy, true);
    unlistenOutput?.();
    unlistenExit?.();
    unlistenWheel?.();
    unsubTheme?.();
    unsubPrefs?.();
    resizeObs?.disconnect();
    if (resizeTimer) clearTimeout(resizeTimer);
    releaseWebgl();
    term?.dispose();
    term = null;
  });

  function loadWebgl() {
    // Bail after repeated consecutive failures — stay on the DOM renderer
    // instead of churning through addons forever. webglFailures resets on a
    // successful load, so this catches a tight failure loop, not the occasional
    // sleep/wake recovery.
    if (!term || webglAddon || !terminalActive || webglFailures >= 4) return;
    if (liveWebglContexts >= MAX_WEBGL_CONTEXTS) return;
    let addon: WebglAddon | null = null;
    try {
      addon = new WebglAddon();
      const loaded = addon;
      loaded.onContextLoss(() => {
        // The GPU context was torn down — most often when the machine wakes
        // from sleep. xterm waits ~3s for the browser to restore it before
        // firing this; once it does, drop the dead addon so xterm falls back
        // to the DOM renderer, refit, and retry WebGL after the GPU settles.
        if (webglAddon !== loaded) return;
        releaseWebgl();
        webglFailures++;
        scheduleFit();
        scheduleWebglRetry(1000);
      });
      term.loadAddon(loaded);
      webglAddon = loaded;
      liveWebglContexts++;
      webglFailures = 0;
      // The first fit is owned by onMount's explicit fitNow(); only schedule one
      // here when re-loading after a context loss (geometry already established).
      if (lastCols !== 0 || lastRows !== 0) scheduleFit();
    } catch (e) {
      console.warn("WebGL addon unavailable, using DOM renderer:", e);
      addon?.dispose();
      webglFailures++;
      scheduleWebglRetry(1000);
    }
  }

  function releaseWebgl() {
    if (webglRetryTimer) {
      clearTimeout(webglRetryTimer);
      webglRetryTimer = null;
    }
    if (!webglAddon) return;
    const addon = webglAddon;
    webglAddon = null;
    liveWebglContexts--;
    addon.dispose();
  }

  function scheduleWebglRetry(delay: number) {
    if (webglRetryTimer) clearTimeout(webglRetryTimer);
    webglRetryTimer = setTimeout(() => {
      webglRetryTimer = null;
      loadWebgl();
    }, delay);
  }

  // Called from +page.svelte. A soft recover (window focus) just refits and
  // repaints. A hard one (system wake, page becoming visible) also drops the
  // WebGL context — WebKit doesn't reliably fire onContextLoss on wake — and
  // paints with the DOM renderer for a moment before re-acquiring WebGL: right
  // after a hibernate the GPU may not be ready, and a context created too early
  // dies again and blanks the pane.
  function recover(hard: boolean) {
    if (!term) return;
    if (hard) {
      webglFailures = 0;
      releaseWebgl();
      if (terminalActive) scheduleWebglRetry(1500);
    }
    fitNow();
    term.refresh(0, term.rows - 1);
  }

  // Claude Code echoes each submitted prompt into the transcript as a line
  // starting with "❯ ". The same glyph marks its live input box (right under a
  // "─" rule) and the selected option of a numbered choice ("❯ 1. Yes"); both
  // are skipped.
  function promptLines(): number[] {
    if (!term) return [];
    const buf = term.buffer.active;
    const out: number[] = [];
    let prev = "";
    for (let i = 0; i < buf.length; i++) {
      const text = buf.getLine(i)?.translateToString(true) ?? "";
      if (
        text.startsWith("❯ ") &&
        text.trim().length > 1 &&
        !prev.startsWith("─") &&
        !/^❯ \d+\. /.test(text)
      ) {
        out.push(i);
      }
      prev = text;
    }
    return out;
  }

  // Puts the previous / next prompt one line below the top of the viewport.
  // From the bottom of the buffer, "previous" starts at the latest prompt,
  // even if it is still on screen.
  function jumpToPrompt(dir: -1 | 1) {
    if (!term) return;
    const buf = term.buffer.active;
    const lines = promptLines();
    const anchor = buf.viewportY + 1;
    const atBottom = buf.viewportY >= buf.baseY;
    let target: number | undefined;
    if (dir < 0) {
      const limit = atBottom ? buf.baseY + term.rows : anchor;
      target = lines.filter((l) => l < limit).pop();
      if (atBottom && target !== undefined && target === anchor) {
        target = lines.filter((l) => l < anchor).pop();
      }
    } else {
      target = lines.find((l) => l > anchor);
    }
    if (target === undefined) {
      if (dir > 0) term.scrollToBottom();
      return;
    }
    term.scrollToLine(Math.max(0, target - 1));
    flashLine(target);
  }

  // Briefly highlights a buffer line so the eye finds the prompt after a jump.
  function flashLine(line: number) {
    if (!term) return;
    const buf = term.buffer.active;
    const marker = term.registerMarker(line - (buf.baseY + buf.cursorY));
    if (!marker) return;
    const color = xtermThemes[get(resolvedTheme)].selectionBackground;
    const deco = term.registerDecoration({
      marker,
      width: term.cols,
      backgroundColor: typeof color === "string" && /^#[0-9a-f]{6}$/i.test(color) ? color : undefined,
    });
    setTimeout(() => {
      deco?.dispose();
      marker.dispose();
    }, 900);
  }

  function scrollToEdge(edge: "top" | "bottom") {
    if (edge === "top") term?.scrollToTop();
    else term?.scrollToBottom();
  }

  // The ⋯ menu: everything that has a shortcut, for this pane and its tab.
  let menuAt = $state<{ x: number; y: number } | null>(null);

  const keys = (mac: string, linux: string) => shortcut(get(platform), mac, linux);
  const report = (what: string) => (err: unknown) => console.error(`${what}:`, err);

  // Only what belongs to the pane: Claude's session, the layout, closing. Tab
  // actions (sub-tab, unread, dev container) live in the card's right-click
  // menu; scrolling, background tasks and artifacts have their own buttons.
  let menuItems = $derived.by((): MenuItem[] => {
    if (!terminal) return [];
    const tid = terminalId;
    const multi = terminal.panes.length > 1;
    const items: MenuItem[] = [];
    const claudeSession = pane?.claude && pane.claude.status !== "ended";
    const files = pane?.claude?.files.length ?? 0;
    if (claudeHere || files > 0) {
      const claudeItems: MenuItem[] = [];
      if (claudeSession) {
        claudeItems.push({
          label: "Fork session into a sub-tab",
          hint: "/fork",
          run: () => api.forkClaudeSession(paneId).catch(report("fork")),
        });
      }
      claudeItems.push({
        label: "Files changed by Claude…",
        hint: keys("⌘⇧S", "Ctrl+Shift+Alt+S"),
        run: () => uiRequest.set({ kind: "files", terminalId: tid, autoStage: false }),
      });
      if (claudeHere) {
        claudeItems.push({
          label: "List your prompts…",
          hint: keys("⌘⇧L", "Ctrl+Shift+Alt+L"),
          run: () => openPromptList(),
        });
      }
      claudeItems[0].section = "Claude";
      items.push(...claudeItems);
    }
    items.push(
      {
        label: "Split side by side",
        hint: keys("⌘D", "Ctrl+Shift+D"),
        section: "Layout",
        run: () => api.splitPane(paneId, "horizontal").catch(report("split")),
      },
      {
        label: "Split stacked",
        hint: keys("⌘⇧D", "Ctrl+Shift+Alt+D"),
        run: () => api.splitPane(paneId, "vertical").catch(report("split")),
      },
    );
    if (multi) {
      items.push(
        {
          label: "Flip split orientation",
          hint: keys("⌘/", "Ctrl+Shift+/"),
          run: () => api.flipParentSplit(paneId).catch(report("flip")),
        },
        {
          label: "Move pane to its own tab",
          hint: keys("⌘⇧E", "Ctrl+Shift+Alt+E"),
          run: () =>
            api
              .extractPane(paneId)
              .then((snap) => selectTerminal(snap.id))
              .catch(report("extract")),
        },
      );
    }
    items.push({
      label: multi ? "Close pane" : "Close tab",
      hint: keys("⌘W", "Ctrl+Shift+W"),
      danger: true,
      dividerBefore: true,
      run: () => uiRequest.set({ kind: "close", terminalId: tid, paneId }),
    });
    return items;
  });

  function openMenu(e: MouseEvent) {
    e.stopPropagation();
    onFocus?.();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menuAt = { x: r.right - 220, y: r.bottom + 4 };
  }

  function closeMenu() {
    menuAt = null;
    term?.focus();
  }

  function handleNav(e: MouseEvent, run: () => void) {
    e.stopPropagation();
    run();
    term?.focus();
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
        // Code-block copy buttons sit at the right edge: redo them for the
        // new width (the TUI may also have redrawn its text).
        if (cols !== lastCols && lastCols !== 0) {
          clearCodeButtons();
          schedulePlacement(codeBlocks);
        }
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

  // Hand the WebGL context back when this terminal is hidden. Loading is
  // deferred a tick so the previously visible terminal releases its contexts
  // first and the newly visible one isn't refused by the cap.
  $effect(() => {
    if (terminalActive) {
      scheduleWebglRetry(0);
    } else {
      releaseWebgl();
    }
  });

  $effect(() => {
    if (active && terminalActive && term) {
      tick().then(() => {
        fitNow();
        term?.focus();
      });
    }
  });

  // --- Background tasks and artifacts reported by the Claude plugin ---------
  let backgroundTasks = $derived(pane?.claude?.background ?? []);
  let artifacts = $derived(pane?.claude?.artifacts ?? []);
  let spyOpen = $state(false);
  let artifactsOpen = $state(false);

  function openArtifact(url: string) {
    artifactsOpen = false;
    openExternal(url).catch((err) => console.error("open artifact:", err));
  }

  // --- Scrollback search (⌘F) ------------------------------------------------
  let searchAddon: SearchAddon | null = null;
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchResult = $state<{ index: number; count: number } | null>(null);
  let searchInput: HTMLInputElement | null = $state(null);

  function searchDecorations() {
    const t = xtermThemes[get(resolvedTheme)];
    const hex = (c: unknown, fallback: string) =>
      typeof c === "string" && /^#[0-9a-f]{6}$/i.test(c) ? c : fallback;
    const match = hex(t.selectionBackground, "#364a82");
    const activeMatch = hex(t.yellow, "#e0af68");
    return {
      matchBackground: match,
      activeMatchBackground: activeMatch,
      matchOverviewRuler: match,
      activeMatchColorOverviewRuler: activeMatch,
    };
  }

  async function openSearch() {
    searchOpen = true;
    await tick();
    searchInput?.focus();
    searchInput?.select();
  }

  function closeSearch() {
    searchOpen = false;
    searchResult = null;
    searchAddon?.clearDecorations();
    term?.focus();
  }

  function runSearch(dir: -1 | 1, incremental = false) {
    if (!searchAddon || !searchQuery) {
      searchResult = null;
      searchAddon?.clearDecorations();
      return;
    }
    const opts = { decorations: searchDecorations(), incremental };
    if (dir > 0) searchAddon.findNext(searchQuery, opts);
    else searchAddon.findPrevious(searchQuery, opts);
  }

  function handleSearchKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      runSearch(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeSearch();
    }
  }

  // --- The list of prompts ---------------------------------------------------
  let promptListOpen = $state(false);
  let promptEntries = $state<{ line: number; text: string }[]>([]);

  // A prompt's text: its "❯ " line plus the lines it wrapped onto.
  function collectPrompts() {
    if (!term) return [];
    const buf = term.buffer.active;
    return promptLines().map((line) => {
      let text = buf.getLine(line)?.translateToString(true).slice(2) ?? "";
      for (let i = line + 1; i < buf.length && buf.getLine(i)?.isWrapped; i++) {
        text += buf.getLine(i)?.translateToString(true) ?? "";
      }
      return { line, text: text.trim() };
    });
  }

  function openPromptList() {
    promptEntries = collectPrompts();
    promptListOpen = true;
  }

  function goToPrompt(line: number) {
    promptListOpen = false;
    term?.scrollToLine(Math.max(0, line - 1));
    flashLine(line);
    term?.focus();
  }

  // --- Clickable file paths (⌘-click) ----------------------------------------
  // "src/app.ts", "./a/b.rs:42", "/abs/path.py:10:3", "main.go:12". A bare
  // name needs a line number; URLs are left to the web-links addon.
  const PATH_RE =
    /(?:~|\.{1,2})?\/?(?:[\w.@+-]+\/)+[\w.@+-]+(?::\d+(?::\d+)?)?|[\w.@+-]+\.[A-Za-z]\w{0,9}:\d+(?::\d+)?/g;

  function registerPathLinks(t: Terminal) {
    t.registerLinkProvider({
      provideLinks(y, callback) {
        const text = t.buffer.active.getLine(y - 1)?.translateToString(true) ?? "";
        const links = [];
        for (const m of text.matchAll(PATH_RE)) {
          const raw = m[0];
          const start = m.index ?? 0;
          const tokenStart = text.lastIndexOf(" ", start) + 1;
          if (text.slice(tokenStart, start + raw.length).includes("://")) continue;
          const parts = /^(.*?)(?::(\d+))?(?::(\d+))?$/.exec(raw);
          if (!parts) continue;
          const path = parts[1].replace(/[.,;:)\]]+$/, "");
          if (!path || /^[\d.]+$/.test(path)) continue;
          const lineNo = parts[2] ? Number(parts[2]) : undefined;
          const colNo = parts[3] ? Number(parts[3]) : undefined;
          links.push({
            range: { start: { x: start + 1, y }, end: { x: start + raw.length, y } },
            text: raw,
            decorations: { underline: true, pointerCursor: true },
            hover: () => (container.title = `${shortcut(get(platform), "⌘", "Ctrl")}-click to open in your editor`),
            leave: () => (container.title = ""),
            activate: (e: MouseEvent) => {
              if (!(e.metaKey || e.ctrlKey)) return;
              api.openPath(paneId, path, lineNo, colNo).catch((err) => console.error("openPath:", err));
            },
          });
        }
        callback(links.length > 0 ? links : undefined);
      },
    });
  }

  // --- Copy buttons beside Claude's code blocks -------------------------------
  // The blocks come from the session transcript (verbatim markdown), so the
  // copy keeps indentation and line breaks the screen rendering loses. Each
  // block is located on screen by its first meaningful line and gets a button
  // at the right edge of its top row; the pane's { } list covers blocks that
  // can't be located (scrolled out, or the screen was redrawn).
  let codeBlocks = $derived(pane?.claude?.codeBlocks ?? []);
  let codeListOpen = $state(false);
  let copiedId = $state<string | null>(null);
  // `lift`: rows between the button's marker and the block's first row.
  type Placed = {
    marker: IMarker;
    deco: IDecoration | undefined;
    lift: number;
    offset: number;
    text: string;
  };
  const placed = new Map<string, Placed>();
  let placeTimer: ReturnType<typeof setTimeout> | null = null;

  // Let the TUI finish drawing before looking for the blocks; look again a
  // little later, since a resumed session reports its blocks before it has
  // redrawn its history, and a turn's last message may land after its Stop.
  function schedulePlacement(blocks: CodeBlock[], delays = [500, 2000, 5000]) {
    if (placeTimer) clearTimeout(placeTimer);
    const [first, ...rest] = delays;
    placeTimer = setTimeout(() => {
      placeCodeButtons(blocks);
      if (rest.length > 0) schedulePlacement(blocks, rest);
    }, first);
  }

  $effect(() => {
    schedulePlacement(codeBlocks);
  });

  function clearCodeButtons() {
    if (placeTimer) clearTimeout(placeTimer);
    for (const p of placed.values()) {
      p.deco?.dispose();
      p.marker.dispose();
    }
    placed.clear();
  }

  // The first line long enough to be recognisable, and its offset in the block.
  function anchorOf(code: string): { offset: number; text: string } | null {
    const lines = code.split("\n");
    for (let k = 0; k < lines.length; k++) {
      const t = lines[k].trim();
      if (t.length >= 6) return { offset: k, text: t.slice(0, 40) };
    }
    return null;
  }

  function placeCodeButtons(blocks: CodeBlock[]) {
    if (!term) return;
    const buf = term.buffer.active;
    // Claude Code marks the first line of a message with a bullet ("● ",
    // "⏺ "); a block that opens the message starts right after it.
    const rows: string[] = [];
    for (let y = 0; y < buf.length; y++) {
      const t = buf.getLine(y)?.translateToString(true).trim() ?? "";
      rows.push(t.replace(/^[●⏺•]\s+/, ""));
    }
    // The TUI repaints lines in place, so a marker can outlive the text it
    // was put on: keep a button only while its block still starts there.
    const used = new Set<number>();
    for (const [id, p] of placed) {
      const stale =
        p.marker.isDisposed ||
        !blocks.some((b) => b.id === id) ||
        !rows[p.marker.line + p.lift + p.offset]?.startsWith(p.text);
      if (stale) {
        p.deco?.dispose();
        p.marker.dispose();
        placed.delete(id);
      } else {
        used.add(p.marker.line + p.lift);
      }
    }
    // Newest blocks first, each matched to the latest screen occurrence not
    // already taken, so a block shown twice (a redraw) gets the copy on screen.
    // A block Claude prints again (same code, same id) moves its button down.
    for (const block of [...blocks].reverse().slice(0, 30)) {
      const anchor = anchorOf(block.code);
      if (!anchor) continue;
      const current = placed.get(block.id);
      const currentTop = current ? current.marker.line + current.lift : -1;
      let found = -1;
      for (let y = rows.length - 1; y >= 0; y--) {
        const top = y - anchor.offset;
        if ((top === currentTop || !used.has(top)) && rows[y].startsWith(anchor.text)) {
          found = y;
          break;
        }
      }
      if (found < 0) continue;
      const top = Math.max(0, found - anchor.offset);
      if (top === currentTop) continue;
      if (current) {
        current.deco?.dispose();
        current.marker.dispose();
        placed.delete(block.id);
        used.delete(currentTop);
      }
      used.add(top);
      // On the blank row above the block when there is one, so the button
      // never covers code.
      const lift = top > 0 && rows[top - 1] === "" ? 1 : 0;
      const marker = term.registerMarker(top - lift - (buf.baseY + buf.cursorY));
      if (!marker) continue;
      // Clear of the prompt-navigation column (24px buttons, 22px from the edge).
      const cellWidth = (term.element?.clientWidth ?? 0) / term.cols || 8;
      const navCols = Math.ceil(50 / cellWidth);
      const deco = term.registerDecoration({
        marker,
        x: Math.max(0, term.cols - 3 - navCols),
        width: 3,
        layer: "top",
      });
      deco?.onRender((el) => {
        if (el.dataset.codeCopy) return;
        el.dataset.codeCopy = "1";
        el.classList.add("code-copy");
        const btn = document.createElement("button");
        btn.type = "button";
        btn.title = "Copy this code block";
        btn.setAttribute("aria-label", "Copy this code block");
        btn.innerHTML =
          '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/></svg>';
        btn.addEventListener("mousedown", (e) => e.stopPropagation());
        btn.addEventListener("click", (e) => {
          e.stopPropagation();
          void copyBlock(block, btn);
        });
        el.appendChild(btn);
      });
      placed.set(block.id, { marker, deco, lift, ...anchor });
    }
  }

  async function copyBlock(block: CodeBlock, btn?: HTMLElement) {
    const ok = await copyToClipboard(block.code);
    if (!ok) return;
    copiedId = block.id;
    btn?.classList.add("copied");
    setTimeout(() => {
      btn?.classList.remove("copied");
      if (copiedId === block.id) copiedId = null;
    }, 1200);
  }

  function firstLine(code: string) {
    return code.split("\n").find((l) => l.trim()) ?? "";
  }

  // --- Copy: one line per line the program meant ----------------------------
  // Set by copyRaw so the next copy event keeps the rows as displayed.
  let copyAsDisplayed = false;

  // The selection, with rows a program wrapped itself (Claude Code breaks long
  // lines with real newlines) joined back. Selection positions are 0-based
  // buffer columns / rows, whatever the typings say.
  function smartSelection(): string | null {
    if (!term || !term.hasSelection()) return null;
    const pos = term.getSelectionPosition();
    if (!pos) return null;
    const buf = term.buffer.active;
    const nearby: string[] = [];
    const from = Math.max(0, pos.start.y - 150);
    const to = Math.min(buf.length - 1, pos.end.y + 150);
    for (let y = from; y <= to; y++) {
      const line = buf.getLine(y);
      // Soft-wrapped rows are fragments of one longer line; skip them.
      if (line && !line.isWrapped) nearby.push(line.translateToString(true));
    }
    const rows: CopyRow[] = [];
    for (let y = pos.start.y; y <= pos.end.y; y++) {
      const line = buf.getLine(y);
      if (!line) continue;
      rows.push({
        full: line.translateToString(true),
        selected: line.translateToString(
          true,
          y === pos.start.y ? pos.start.x : 0,
          y === pos.end.y ? pos.end.x : undefined,
        ),
        wrapped: line.isWrapped,
      });
    }
    return joinRows(rows, wrapWidth(nearby, term.cols));
  }

  function handleCopy(e: ClipboardEvent) {
    if (!term?.hasSelection() || !e.clipboardData) return;
    const text = copyAsDisplayed ? term.getSelection() : smartSelection();
    copyAsDisplayed = false;
    if (text === null) return;
    e.clipboardData.setData("text/plain", text);
    e.preventDefault();
    e.stopImmediatePropagation();
  }

  function copyRaw() {
    if (!term?.hasSelection()) return;
    copyAsDisplayed = true;
    term.focus();
    document.execCommand("copy");
    copyAsDisplayed = false;
  }

  // Clicking or focusing the pane activates it and hands focus to xterm —
  // except inside the pane's own controls (search field, menus, buttons),
  // which would otherwise lose focus the moment they got it.
  function handleHostClick(e: Event) {
    onFocus?.();
    const target = e.target as Element | null;
    if (target?.closest(".search-bar, .popover, .pane-tools, .prompt-nav, .menu, .backdrop")) {
      return;
    }
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
>
  <div class="pane-tools" class:pinned={menuAt !== null || artifactsOpen || searchOpen}>
    {#if artifacts.length > 0}
      <button
        class="tool-btn"
        class:open={artifactsOpen}
        title="Artifacts Claude published"
        aria-label="Artifacts Claude published"
        onclick={(e) => {
          e.stopPropagation();
          artifactsOpen = !artifactsOpen;
        }}
      >
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M14 3h7v7" />
          <path d="M10 14 21 3" />
          <path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5" />
        </svg>
        <span class="count">{artifacts.length}</span>
      </button>
    {/if}
    {#if codeBlocks.length > 0}
      <button
        class="tool-btn"
        class:open={codeListOpen}
        title="Code blocks from Claude — copy them exactly as written"
        aria-label="Code blocks from Claude"
        onclick={(e) => {
          e.stopPropagation();
          codeListOpen = !codeListOpen;
        }}
      >
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="16 18 22 12 16 6" />
          <polyline points="8 6 2 12 8 18" />
        </svg>
        <span class="count">{codeBlocks.length}</span>
      </button>
    {/if}
    {#if backgroundTasks.length > 0}
      <button
        class="tool-btn spy"
        title="See what Claude is running in the background"
        aria-label="See what Claude is running in the background"
        onclick={(e) => {
          e.stopPropagation();
          spyOpen = true;
        }}
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M3 11h18" />
          <path d="M6 11l1.5-6h9L18 11" />
          <circle cx="7.5" cy="16.5" r="2.5" />
          <circle cx="16.5" cy="16.5" r="2.5" />
          <path d="M10 16.5h4" />
        </svg>
        <span class="count">{backgroundTasks.length}</span>
      </button>
    {/if}
    <button
      class="tool-btn"
      class:open={searchOpen}
      title={`Find in scrollback (${shortcut($platform, "⌘F", "Ctrl+Shift+F")})`}
      aria-label="Find in scrollback"
      onclick={(e) => {
        e.stopPropagation();
        if (searchOpen) closeSearch();
        else void openSearch();
      }}
    >
      <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <circle cx="11" cy="11" r="7" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
    </button>
    <button
      class="tool-btn"
      class:open={menuAt !== null}
      title="Pane actions"
      aria-label="Pane actions"
      aria-haspopup="menu"
      onclick={openMenu}
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" aria-hidden="true">
        <circle cx="5" cy="12" r="1.8" />
        <circle cx="12" cy="12" r="1.8" />
        <circle cx="19" cy="12" r="1.8" />
      </svg>
    </button>
  </div>
  {#if artifactsOpen}
    <div class="popover artifacts" role="menu" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === "Escape" && (artifactsOpen = false)} tabindex="-1">
      <div class="popover-title">Artifacts from this session</div>
      {#each [...artifacts].reverse() as a (a.url)}
        <button class="popover-item" role="menuitem" title={a.url} onclick={() => openArtifact(a.url)}>
          <span class="item-title">{a.title}</span>
          <span class="item-sub">{a.url.replace(/^https:\/\//, "")}</span>
        </button>
      {/each}
    </div>
  {/if}
  {#if codeListOpen}
    <div class="popover artifacts" role="menu" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === "Escape" && (codeListOpen = false)} tabindex="-1">
      <div class="popover-title">Code blocks from Claude</div>
      {#each [...codeBlocks].reverse() as b (b.id)}
        <button class="popover-item" role="menuitem" title={b.code} onclick={() => copyBlock(b)}>
          <span class="item-title code-line">{firstLine(b.code)}</span>
          <span class="item-sub">
            {copiedId === b.id ? "Copied" : `${b.lang || "code"} · ${b.code.split("\n").length} lines · click to copy`}
          </span>
        </button>
      {/each}
    </div>
  {/if}
  {#if searchOpen}
    <div class="search-bar" role="presentation" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <input
        bind:this={searchInput}
        bind:value={searchQuery}
        type="text"
        placeholder="Find in scrollback"
        aria-label="Find in scrollback"
        oninput={() => runSearch(1, true)}
        onkeydown={handleSearchKey}
      />
      <span class="search-count">
        {#if searchQuery && searchResult}
          {searchResult.count === 0 ? "no matches" : `${searchResult.index + 1} of ${searchResult.count}`}
        {/if}
      </span>
      <button title="Previous match (⇧Enter)" aria-label="Previous match" onclick={() => runSearch(-1)}>
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="18 15 12 9 6 15" /></svg>
      </button>
      <button title="Next match (Enter)" aria-label="Next match" onclick={() => runSearch(1)}>
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 9 12 15 18 9" /></svg>
      </button>
      <button title="Close (Esc)" aria-label="Close search" onclick={closeSearch}>×</button>
    </div>
  {/if}
  {#if spyOpen}
    <BackgroundDialog {paneId} tasks={backgroundTasks} onClose={() => {
      spyOpen = false;
      term?.focus();
    }} />
  {/if}
  {#if menuAt}
    <TabContextMenu x={menuAt.x} y={menuAt.y} items={menuItems} onClose={closeMenu} />
  {/if}
  {#if claudeHere}
    <div class="prompt-nav" role="group" aria-label="Scroll through the conversation">
      <button
        title={`Go to the start (${shortcut($platform, "⌘Home", "Ctrl+Shift+Home")})`}
        aria-label="Go to the start"
        onclick={(e) => handleNav(e, () => scrollToEdge("top"))}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <line x1="6" y1="5" x2="18" y2="5" />
          <polyline points="18 17 12 11 6 17" />
        </svg>
      </button>
      <button
        title={`Previous prompt to Claude (${shortcut($platform, "⌘↑", "Ctrl+Shift+↑")})`}
        aria-label="Previous prompt to Claude"
        onclick={(e) => handleNav(e, () => jumpToPrompt(-1))}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="18 15 12 9 6 15" />
        </svg>
      </button>
      <button
        title={`List your prompts (${shortcut($platform, "⌘⇧L", "Ctrl+Shift+Alt+L")})`}
        aria-label="List your prompts"
        onclick={(e) => {
          e.stopPropagation();
          if (promptListOpen) promptListOpen = false;
          else openPromptList();
        }}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <line x1="9" y1="6" x2="20" y2="6" />
          <line x1="9" y1="12" x2="20" y2="12" />
          <line x1="9" y1="18" x2="20" y2="18" />
          <circle cx="4.5" cy="6" r="0.6" />
          <circle cx="4.5" cy="12" r="0.6" />
          <circle cx="4.5" cy="18" r="0.6" />
        </svg>
      </button>
      <button
        title={`Next prompt to Claude (${shortcut($platform, "⌘↓", "Ctrl+Shift+↓")})`}
        aria-label="Next prompt to Claude"
        onclick={(e) => handleNav(e, () => jumpToPrompt(1))}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
      <button
        title={`Go to the end (${shortcut($platform, "⌘End", "Ctrl+Shift+End")})`}
        aria-label="Go to the end"
        onclick={(e) => handleNav(e, () => scrollToEdge("bottom"))}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="6 7 12 13 18 7" />
          <line x1="6" y1="19" x2="18" y2="19" />
        </svg>
      </button>
    </div>
    {#if promptListOpen}
      <div class="popover prompts" role="menu" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === "Escape" && (promptListOpen = false)} tabindex="-1">
        <div class="popover-title">Your prompts in this pane</div>
        {#if promptEntries.length === 0}
          <div class="popover-empty">No prompts found in the scrollback.</div>
        {/if}
        {#each promptEntries as p, i (p.line)}
          <button class="popover-item" role="menuitem" title={p.text} onclick={() => goToPrompt(p.line)}>
            <span class="item-num">{i + 1}</span>
            <span class="item-title">{p.text}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
</div>

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

  /* Top-right cluster: artifacts, background tasks, the ⋯ menu. */
  .pane-tools {
    position: absolute;
    top: 10px;
    right: 22px;
    z-index: 20;
    display: flex;
    gap: 4px;
    opacity: 0.35;
    transition: opacity 0.12s;
  }

  .xterm-host:hover .pane-tools,
  .pane-tools.pinned,
  .pane-tools:focus-within {
    opacity: 0.95;
  }

  .tool-btn {
    height: 22px;
    min-width: 26px;
    padding: 0 5px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    color: var(--fg);
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    cursor: pointer;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .tool-btn:hover,
  .tool-btn.open {
    background: var(--bg-hover);
  }

  /* Background work is worth noticing even when the pane isn't hovered. */
  .tool-btn.spy {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
  }

  .popover {
    position: absolute;
    z-index: 25;
    width: min(380px, 70%);
    max-height: 60%;
    overflow-y: auto;
    padding: 4px;
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }

  .popover.artifacts {
    top: 38px;
    right: 22px;
  }

  .popover.prompts {
    right: 56px;
    top: 50%;
    transform: translateY(-50%);
  }

  .popover-title {
    padding: 6px 9px 4px;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }

  .popover-empty {
    padding: 6px 9px 8px;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .popover-item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    width: 100%;
    padding: 6px 9px;
    border-radius: 5px;
    text-align: left;
    color: var(--fg);
    min-width: 0;
  }

  .popover.prompts .popover-item {
    flex-direction: row;
    align-items: baseline;
    gap: 8px;
  }

  .popover-item:hover {
    background: var(--bg-hover);
  }

  .item-title {
    font-size: 12.5px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .item-sub {
    font-size: 11px;
    color: var(--fg-muted);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-num {
    flex-shrink: 0;
    font-size: 11px;
    color: var(--fg-faint);
    font-variant-numeric: tabular-nums;
    min-width: 16px;
    text-align: right;
  }

  .code-line {
    font-family: Menlo, Monaco, monospace;
    font-size: 11.5px;
  }

  /* Copy button xterm draws as a decoration on a code block's top row. */
  :global(.code-copy) {
    display: flex;
    justify-content: flex-end;
    pointer-events: none;
  }

  :global(.code-copy button) {
    pointer-events: auto;
    width: 22px;
    height: 18px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--fg);
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    opacity: 0.5;
    cursor: pointer;
    transition: opacity 0.12s, background 0.12s;
  }

  :global(.code-copy button:hover) {
    opacity: 1;
    background: var(--bg-hover);
  }

  :global(.code-copy button.copied) {
    opacity: 1;
    color: #2ea043;
    border-color: #2ea043;
  }

  .search-bar {
    position: absolute;
    top: 38px;
    right: 22px;
    z-index: 25;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px;
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }

  .search-bar input {
    width: 200px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 7px;
    color: var(--fg);
    font-size: 12.5px;
    outline: none;
  }

  .search-bar input:focus {
    border-color: var(--accent);
  }

  .search-count {
    min-width: 64px;
    font-size: 11px;
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .search-bar button {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--fg);
    font-size: 15px;
  }

  .search-bar button:hover {
    background: var(--bg-hover);
  }

  /* Sits left of the 12px scrollbar, above the canvas and the viewport. */
  .prompt-nav {
    position: absolute;
    right: 22px;
    top: 50%;
    transform: translateY(-50%);
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 4px;
    opacity: 0.45;
    transition: opacity 0.12s;
  }

  .xterm-host:hover .prompt-nav,
  .prompt-nav:focus-within {
    opacity: 0.9;
  }

  .prompt-nav button {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--fg);
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
    cursor: pointer;
  }

  .prompt-nav button:hover {
    background: var(--bg-hover);
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
