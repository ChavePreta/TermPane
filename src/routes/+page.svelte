<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import "@xterm/xterm/css/xterm.css";
  import "$lib/styles/app.css";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import PaneTreeView from "$lib/components/PaneTreeView.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import PreferencesDialog from "$lib/components/PreferencesDialog.svelte";
  import ShortcutsDialog from "$lib/components/ShortcutsDialog.svelte";
  import FilesDialog from "$lib/components/FilesDialog.svelte";
  import ResumeDialog from "$lib/components/ResumeDialog.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import KeyboardShortcutsDialog from "$lib/components/KeyboardShortcutsDialog.svelte";
  import {
    terminals,
    activeId,
    aggregateForeground,
    selectTerminal,
    needsYou,
  } from "$lib/store";
  import { clearAttention, signalNotification, attentionTerminals } from "$lib/attention";
  import { clearProgress } from "$lib/progress";
  import type { ClaudeStatus } from "$lib/api";
  import {
    loadPreferences,
    installPreferencesListener,
  } from "$lib/preferences";
  import { loadShortcuts, installShortcutsListener } from "$lib/shortcuts";
  import { alwaysOnTop } from "$lib/alwaysOnTop";
  import { sidebarCollapsed } from "$lib/sidebarLayout";
  import { platform, initPlatform, shortcut } from "$lib/platform";
  import { broadcastEnabled, disableBroadcast } from "$lib/broadcast";
  import { get } from "svelte/store";
  import {
    api,
    onTerminalAdded,
    onTerminalRemoved,
    onTerminalUpdated,
    onPaneForeground,
    onPaneStats,
    onTerminalsReordered,
    onAppCloseRequested,
    onSystemWoke,
    onPaneClaude,
    onTerminalFocusRequested,
    onShowShortcutsRequested,
    onPaneContext,
  } from "$lib/api";
  import { getPaneActions, recoverAllPanes } from "$lib/paneActions";
  import { uiRequest } from "$lib/uiRequests";
  import { preferences, savePreferences } from "$lib/preferences";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  type PendingAction =
    | { kind: "terminal"; terminalId: string }
    | { kind: "pane"; terminalId: string; paneId: string };

  let pending = $state<PendingAction | null>(null);
  let prefsOpen = $state(false);
  let shortcutsOpen = $state(false);
  let filesFor = $state<string | null>(null);
  let filesAutoStage = $state(false);
  let resumeOpen = $state(false);
  let keysOpen = $state(false);
  let quitRunning = $state<string[] | null>(null);
  let unlisteners: UnlistenFn[] = [];

  let quitMessage = $derived.by(() => {
    if (!quitRunning || quitRunning.length === 0) return "";
    const list = quitRunning.map((r) => `  • ${r}`).join("\n");
    const verb = quitRunning.length === 1 ? "command is" : "commands are";
    return `${quitRunning.length} ${verb} still running:\n${list}\n\nQuit anyway?`;
  });

  let pendingTerminal = $derived(
    pending ? $terminals.find((t) => t.id === pending!.terminalId) ?? null : null,
  );

  // The user is now looking at the active terminal — drop any attention flag on
  // it. Fires for every selection path (sidebar click, pane focus, keyboard
  // switch, new terminal) since they all funnel through $activeId.
  $effect(() => {
    const id = $activeId;
    if (id) clearAttention(id);
  });

  // A bell can flag the active terminal while the window is backgrounded; clear
  // it once the window regains focus and the user is looking again. Regaining
  // focus is also a wake signal, so repaint every pane (see handleVisibility).
  function handleWindowFocus() {
    const id = get(activeId);
    if (id) clearAttention(id);
    recoverAllPanes(false);
  }

  // WKWebView maps window occlusion (display sleep, hibernate, minimize) to the
  // Page Visibility API, so a wake surfaces as visibilitychange -> "visible".
  // On wake the GPU context can be silently lost or the compositor can leave
  // panes painted white; force a full repaint + WebGL recreation on each pane.
  function handleVisibility() {
    if (document.visibilityState === "visible") recoverAllPanes(true);
  }

  // Dock badge: tabs that need the user — flagged by a bell / notification
  // while unwatched, or Claude asking for permission / input.
  let badgeCount = $derived(
    $terminals.filter(
      (t) => $attentionTerminals.get(t.id)?.persistent || needsYou(t),
    ).length,
  );
  let lastBadge = -1;
  $effect(() => {
    const n = badgeCount;
    if (n === lastBadge) return;
    lastBadge = n;
    api.setBadgeCount(n).catch((e) => console.error("setBadgeCount:", e));
  });

  // Requests from a pane's ⋯ menu for UI this page owns.
  $effect(() => {
    const req = $uiRequest;
    if (!req) return;
    uiRequest.set(null);
    if (req.kind === "files") {
      filesAutoStage = req.autoStage;
      filesFor = req.terminalId;
    } else {
      const t = get(terminals).find((x) => x.id === req.terminalId);
      if (!t) return;
      pending =
        t.panes.length > 1
          ? { kind: "pane", terminalId: t.id, paneId: req.paneId }
          : { kind: "terminal", terminalId: t.id };
    }
  });

  // Claude moving into a state that wants the user flags the card like a bell
  // (pulse, persistent dot when unwatched, OS notification when unfocused).
  const CLAUDE_ALERTS: Partial<Record<ClaudeStatus, string>> = {
    permission: "Claude needs your permission",
    waiting: "Claude is waiting for your input",
    done: "Claude finished",
  };
  const lastClaudeStatus = new Map<string, ClaudeStatus>();

  function handleClaudeChange(terminalId: string, paneId: string, status: ClaudeStatus | null) {
    const prev = lastClaudeStatus.get(paneId);
    if (status) lastClaudeStatus.set(paneId, status);
    else lastClaudeStatus.delete(paneId);
    if (!status || status === prev) return;
    const alert = CLAUDE_ALERTS[status];
    if (alert) signalNotification(terminalId, alert);
  }

  async function openSubTab(parentId: string) {
    try {
      const snap = await api.openTerminal({ parentId });
      await selectTerminal(snap.id);
    } catch (err) {
      console.error("open sub-tab:", err);
    }
  }

  // Next tab, after the active one (wrapping), that wants the user: marked
  // unread, flagged by a bell/notification, or Claude asking for input.
  async function jumpToNextAttention() {
    const list = get(terminals);
    if (list.length === 0) return;
    const flags = get(attentionTerminals);
    const start = list.findIndex((t) => t.id === get(activeId));
    for (let i = 1; i < list.length; i++) {
      const t = list[(start + i + list.length) % list.length];
      if (t.markedUnread || flags.get(t.id)?.persistent || needsYou(t)) {
        await selectTerminal(t.id);
        return;
      }
    }
  }

  let confirmTitle = $derived(
    pending?.kind === "pane" ? "Close pane" : "Close terminal",
  );

  let confirmMessage = $derived.by(() => {
    if (!pendingTerminal || !pending) return "";
    const action = pending;
    if (action.kind === "pane") {
      const p = pendingTerminal.panes.find((x) => x.id === action.paneId);
      const running = p?.foregroundCommand
        ? `\nRunning: ${p.foregroundCommand}`
        : "";
      return `Close the current pane of ${pendingTerminal.label}?${running}`;
    }
    const fg = aggregateForeground(pendingTerminal);
    const paneCount = pendingTerminal.panes.length;
    const running = fg ? `\nRunning: ${fg}` : "";
    const panes = paneCount > 1 ? `\n${paneCount} panes open.` : "";
    const subTabs = $terminals.filter((t) => t.parentId === pendingTerminal.id).length;
    const nested =
      subTabs > 0
        ? `\nIts ${subTabs} sub-tab${subTabs === 1 ? "" : "s"} will stay open and move up a level.`
        : "";
    return `Are you sure you want to close ${pendingTerminal.label}?${panes}${running}${nested}`;
  });

  onMount(async () => {
    await initPlatform();
    installPreferencesListener();
    await loadPreferences();
    await installShortcutsListener();
    await loadShortcuts();
    // Sync the window with the persisted always-on-top state.
    try {
      await api.setAlwaysOnTop(get(alwaysOnTop));
    } catch (e) {
      console.error("sync alwaysOnTop:", e);
    }

    unlisteners.push(
      await onTerminalAdded((t) => terminals.add(t)),
      await onTerminalRemoved((id) => {
        terminals.remove(id);
        disableBroadcast(id);
      }),
      await onTerminalUpdated((t) => {
        terminals.upsert(t);
        if (t.panes.length < 2) disableBroadcast(t.id);
      }),
      await onPaneForeground(({ terminalId, paneId, command }) => {
        terminals.setPaneForeground(terminalId, paneId, command);
        // Back at the prompt: a bar the program never cleared goes too.
        if (command === null) clearProgress(paneId);
      }),
      await onPaneStats((stats) => terminals.setPaneStats(stats)),
      await onTerminalsReordered((ids) => terminals.reorder(ids)),
      await onAppCloseRequested((running) => {
        quitRunning = running;
      }),
      // A macOS system wake. Repaint every pane from inside a requestAnimationFrame
      // — which only fires once the compositor is painting again — then ack the
      // backend. If the WebContent process died on hibernate, JS never runs this
      // and no ack arrives, so the backend reloads the webview as a last resort
      // (see wake.rs). The rAF ack is what preserves scrollback in the common,
      // recoverable case instead of forcing a reload every wake.
      await onSystemWoke((generation) => {
        requestAnimationFrame(() => {
          recoverAllPanes(true);
          api.ackWoke(generation).catch((e) => console.error("ackWoke:", e));
        });
      }),
      await onPaneClaude(({ terminalId, paneId, claude }) => {
        terminals.setPaneClaude(terminalId, paneId, claude);
        handleClaudeChange(terminalId, paneId, claude?.status ?? null);
      }),
      await onTerminalFocusRequested((id) => void selectTerminal(id)),
      await onShowShortcutsRequested(() => (keysOpen = true)),
      await onPaneContext((c) => terminals.setPaneContext(c)),
    );

    const list = await api.listTerminals();
    terminals.set(list);
    for (const t of list) {
      for (const p of t.panes) if (p.claude) lastClaudeStatus.set(p.id, p.claude.status);
    }
    const active = await api.activeTerminal();
    if (active) activeId.set(active);
    else if (list[0]) activeId.set(list[0].id);

    window.addEventListener("keydown", handleKeydown, { capture: true });
    window.addEventListener("contextmenu", handleContextMenu);
    window.addEventListener("focus", handleWindowFocus);
    document.addEventListener("visibilitychange", handleVisibility);
  });

  onDestroy(() => {
    for (const u of unlisteners) u();
    window.removeEventListener("keydown", handleKeydown, { capture: true });
    window.removeEventListener("contextmenu", handleContextMenu);
    window.removeEventListener("focus", handleWindowFocus);
    document.removeEventListener("visibilitychange", handleVisibility);
  });

  function handleContextMenu(e: MouseEvent) {
    // Allow the native WebView menu inside terminal panes (so users can
    // still get Copy/Paste/Inspect/etc.). Suppress everywhere else — that's
    // where accidental Reload clicks happen.
    if (e.target instanceof Element && e.target.closest(".xterm-host")) {
      return;
    }
    e.preventDefault();
  }

  async function handleKeydown(e: KeyboardEvent) {
    const term = $terminals.find((t) => t.id === $activeId);
    const isMac = get(platform) === "mac";

    // Flip parent split orientation: Mac ⌘/, Linux Ctrl+Shift+/.
    const flipMatch = isMac
      ? e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey
      : e.ctrlKey && e.shiftKey && !e.metaKey && !e.altKey;
    if (flipMatch && (e.key === "/" || e.code === "Slash")) {
      e.preventDefault();
      e.stopPropagation();
      if (term) {
        try {
          await api.flipParentSplit(term.activePane);
        } catch (err) {
          console.error("flipParentSplit erro:", err);
        }
      }
      return;
    }

    // App-mod: Mac uses Cmd; Linux uses Ctrl+Shift. The sub-modifier (Mac's
    // Shift, e.g. ⌘⇧D) maps to Alt on Linux (Ctrl+Shift+Alt+D), since Shift is
    // already part of the Linux base — so Alt must NOT be excluded here, or
    // every sub-modifier shortcut (split-vertical, extract, switch) is dead.
    const isAppMod = isMac
      ? e.metaKey && !e.ctrlKey && !e.altKey
      : e.ctrlKey && e.shiftKey && !e.metaKey;
    const subShift = isMac ? e.shiftKey : e.altKey;

    if (isAppMod) {
      if (!term) return;
      if (e.key === "d" || e.key === "D") {
        e.preventDefault();
        e.stopPropagation();
        const dir = subShift ? "vertical" : "horizontal";
        try {
          await api.splitPane(term.activePane, dir);
        } catch (err) {
          console.error("split erro:", err);
        }
        return;
      }
      if (e.key === "w" || e.key === "W") {
        e.preventDefault();
        e.stopPropagation();
        if (term.panes.length === 1) {
          pending = { kind: "terminal", terminalId: term.id };
        } else {
          pending = {
            kind: "pane",
            terminalId: term.id,
            paneId: term.activePane,
          };
        }
        return;
      }
      if (e.key === "t" || e.key === "T") {
        e.preventDefault();
        e.stopPropagation();
        if (subShift) {
          await openSubTab(term.id);
          return;
        }
        try {
          const snap = await api.openTerminal();
          activeId.set(snap.id);
        } catch (err) {
          console.error("new terminal erro:", err);
        }
        return;
      }
      if (subShift && e.code === "KeyU") {
        e.preventDefault();
        e.stopPropagation();
        api
          .setTerminalUnread(term.id, !term.markedUnread)
          .catch((err) => console.error("setTerminalUnread:", err));
        return;
      }
      if (subShift && e.code === "KeyS") {
        e.preventDefault();
        e.stopPropagation();
        filesAutoStage = true;
        filesFor = term.id;
        return;
      }
      if (subShift && e.code === "KeyO") {
        e.preventDefault();
        e.stopPropagation();
        resumeOpen = true;
        return;
      }
      // Previous / next prompt typed to Claude in the active pane:
      // Mac ⌘↑/⌘↓, Linux Ctrl+Shift+↑/↓.
      if (!subShift && (e.code === "ArrowUp" || e.code === "ArrowDown")) {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.jumpToPrompt(e.code === "ArrowUp" ? -1 : 1);
        return;
      }
      // Copy exactly as displayed (plain copy joins rows a program wrapped):
      // Mac ⌘⇧C, Linux Ctrl+Shift+Alt+C.
      if (subShift && e.code === "KeyC") {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.copyRaw();
        return;
      }
      // Find in scrollback: Mac ⌘F / ⌘G / ⌘⇧G, Linux Ctrl+Shift+F / G / +Alt+G.
      if (!subShift && e.code === "KeyF") {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.openSearch();
        return;
      }
      if (e.code === "KeyG") {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.searchStep(subShift ? -1 : 1);
        return;
      }
      if (subShift && e.code === "KeyL") {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.togglePromptList();
        return;
      }
      // Start / end of the scrollback: Mac ⌘Home/⌘End, Linux Ctrl+Shift+Home/End.
      if (!subShift && (e.code === "Home" || e.code === "End")) {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.scrollToEdge(e.code === "Home" ? "top" : "bottom");
        return;
      }
      if (!subShift && e.code === "KeyJ") {
        e.preventDefault();
        e.stopPropagation();
        await jumpToNextAttention();
        return;
      }
      if (e.key === "k" || e.key === "K") {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.clear();
        return;
      }
      // Switch terminal card: Mac ⌘⇧→/←, Linux Ctrl+Shift+Alt+→/←. Wraps around.
      if (subShift && (e.code === "ArrowRight" || e.code === "ArrowLeft")) {
        e.preventDefault();
        e.stopPropagation();
        const list = $terminals;
        if (list.length < 2) return;
        const idx = list.findIndex((t) => t.id === $activeId);
        const cur = idx < 0 ? 0 : idx;
        const delta = e.code === "ArrowRight" ? 1 : -1;
        const next = list[(cur + delta + list.length) % list.length];
        await selectTerminal(next.id);
        return;
      }
      // Font zoom: Cmd+=/+, Cmd+-, Cmd+0 on Mac; same letters under Ctrl+Shift on Linux.
      if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        e.stopPropagation();
        const p = get(preferences);
        await savePreferences({
          ...p,
          fontSize: Math.min(p.fontSize + 1, 32),
        });
        return;
      }
      if (e.key === "-" || e.key === "_") {
        e.preventDefault();
        e.stopPropagation();
        const p = get(preferences);
        await savePreferences({
          ...p,
          fontSize: Math.max(p.fontSize - 1, 8),
        });
        return;
      }
      if (e.key === "0") {
        e.preventDefault();
        e.stopPropagation();
        const p = get(preferences);
        await savePreferences({ ...p, fontSize: 13 });
        return;
      }
      if (e.key === "b" || e.key === "B") {
        e.preventDefault();
        e.stopPropagation();
        sidebarCollapsed.update((v) => !v);
        return;
      }
      if (subShift && (e.key === "e" || e.key === "E")) {
        e.preventDefault();
        e.stopPropagation();
        if (term.panes.length < 2) return;
        try {
          const snap = await api.extractPane(term.activePane);
          activeId.set(snap.id);
        } catch (err) {
          console.error("extractPane erro:", err);
        }
        return;
      }
    }

    // Ctrl+letter (no Cmd, no Alt, no Shift): send the control byte straight to
    // the PTY. We use e.code (physical, layout-independent) because e.key may
    // come empty/Dead depending on WebKit/keyboard. Honor broadcast mode so
    // Ctrl+C / Ctrl+R / etc. reach every pane when it's on.
    if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && term) {
      const m = e.code.match(/^Key([A-Z])$/);
      if (m) {
        const code = m[1].charCodeAt(0) - 64; // Ctrl+A=1, Ctrl+R=18, Ctrl+Z=26
        e.preventDefault();
        e.stopPropagation();
        const data = String.fromCharCode(code);
        if (get(broadcastEnabled).has(term.id)) {
          api
            .writeInputBroadcast(term.id, data)
            .catch((err) => console.error("ctrl broadcast erro:", err));
        } else {
          api
            .writeInput(term.activePane, data)
            .catch((err) => console.error("ctrl input erro:", err));
        }
      }
    }
  }

  async function confirmAction() {
    if (!pending) return;
    const action = pending;
    pending = null;
    try {
      if (action.kind === "terminal") {
        await api.closeTerminal(action.terminalId);
      } else {
        await api.closePane(action.paneId);
      }
    } catch (e) {
      console.error("confirm action erro:", e);
    }
  }

  function cancelAction() {
    pending = null;
  }

  async function confirmQuit() {
    quitRunning = null;
    try {
      await api.quitApp();
    } catch (e) {
      console.error("quit_app erro:", e);
    }
  }

  function cancelQuit() {
    quitRunning = null;
  }
</script>

<main class="app">
  {#if !$sidebarCollapsed}
    <Sidebar
      onRequestClose={(id) =>
        (pending = { kind: "terminal", terminalId: id })}
      onOpenPreferences={() => (prefsOpen = true)}
      onManageShortcuts={() => (shortcutsOpen = true)}
      onOpenFiles={(id) => {
        filesAutoStage = false;
        filesFor = id;
      }}
      onOpenResume={() => (resumeOpen = true)}
    />
  {/if}
  <section class="content">
    {#if $sidebarCollapsed}
      <button
        class="sidebar-reveal"
        title={`Show sidebar (${shortcut($platform, "⌘B", "Ctrl+Shift+B")})`}
        aria-label="Show sidebar"
        onclick={() => sidebarCollapsed.set(false)}
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="9 18 15 12 9 6" />
        </svg>
      </button>
    {/if}
    <div class="pane-host">
      {#each $terminals as t (t.id)}
        <div
          class="pane"
          class:active={t.id === $activeId}
          class:broadcast-on={$broadcastEnabled.has(t.id) && t.panes.length >= 2}
        >
          <PaneTreeView
            node={t.layout}
            terminal={t}
            terminalActive={t.id === $activeId}
          />
        </div>
      {/each}
    </div>
    <StatusBar onShowShortcuts={() => (keysOpen = true)} />
  </section>
</main>

<ConfirmDialog
  open={pending !== null}
  title={confirmTitle}
  message={confirmMessage}
  confirmLabel="Close"
  onConfirm={confirmAction}
  onCancel={cancelAction}
/>

<ConfirmDialog
  open={quitRunning !== null}
  title="Quit TermPane?"
  message={quitMessage}
  confirmLabel="Quit"
  cancelLabel="Cancel"
  onConfirm={confirmQuit}
  onCancel={cancelQuit}
/>

<PreferencesDialog open={prefsOpen} onClose={() => (prefsOpen = false)} />

<ShortcutsDialog open={shortcutsOpen} onClose={() => (shortcutsOpen = false)} />

<FilesDialog
  terminalId={filesFor}
  autoStage={filesAutoStage}
  onClose={() => (filesFor = null)}
/>

<ResumeDialog open={resumeOpen} onClose={() => (resumeOpen = false)} />

<KeyboardShortcutsDialog open={keysOpen} onClose={() => (keysOpen = false)} />
