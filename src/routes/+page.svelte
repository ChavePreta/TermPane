<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import "@xterm/xterm/css/xterm.css";
  import "$lib/styles/app.css";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import PaneTreeView from "$lib/components/PaneTreeView.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import PreferencesDialog from "$lib/components/PreferencesDialog.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import { terminals, activeId, aggregateForeground } from "$lib/store";
  import {
    loadPreferences,
    installPreferencesListener,
  } from "$lib/preferences";
  import { alwaysOnTop } from "$lib/alwaysOnTop";
  import { get } from "svelte/store";
  import {
    api,
    onTerminalAdded,
    onTerminalRemoved,
    onTerminalUpdated,
    onPaneForeground,
    onTerminalsReordered,
  } from "$lib/api";
  import { getPaneActions } from "$lib/paneActions";
  import { preferences, savePreferences } from "$lib/preferences";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  type PendingAction =
    | { kind: "terminal"; terminalId: string }
    | { kind: "pane"; terminalId: string; paneId: string };

  let pending = $state<PendingAction | null>(null);
  let prefsOpen = $state(false);
  let unlisteners: UnlistenFn[] = [];

  let pendingTerminal = $derived(
    pending ? $terminals.find((t) => t.id === pending!.terminalId) ?? null : null,
  );

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
    return `Are you sure you want to close ${pendingTerminal.label}?${panes}${running}`;
  });

  onMount(async () => {
    installPreferencesListener();
    await loadPreferences();
    // Sync the window with the persisted always-on-top state.
    try {
      await api.setAlwaysOnTop(get(alwaysOnTop));
    } catch (e) {
      console.error("sync alwaysOnTop:", e);
    }

    unlisteners.push(
      await onTerminalAdded((t) => terminals.add(t)),
      await onTerminalRemoved((id) => terminals.remove(id)),
      await onTerminalUpdated((t) => terminals.upsert(t)),
      await onPaneForeground(({ terminalId, paneId, command }) =>
        terminals.setPaneForeground(terminalId, paneId, command),
      ),
      await onTerminalsReordered((ids) => terminals.reorder(ids)),
    );

    const list = await api.listTerminals();
    terminals.set(list);
    const active = await api.activeTerminal();
    if (active) activeId.set(active);
    else if (list[0]) activeId.set(list[0].id);

    window.addEventListener("keydown", handleKeydown, { capture: true });
  });

  onDestroy(() => {
    for (const u of unlisteners) u();
    window.removeEventListener("keydown", handleKeydown, { capture: true });
  });

  async function handleKeydown(e: KeyboardEvent) {
    const term = $terminals.find((t) => t.id === $activeId);

    // App shortcuts (Cmd-modified).
    if (e.metaKey && !e.altKey) {
      if (!term) return;
      if (e.key === "d" || e.key === "D") {
        e.preventDefault();
        e.stopPropagation();
        const dir = e.shiftKey ? "vertical" : "horizontal";
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
        try {
          const snap = await api.openTerminal();
          activeId.set(snap.id);
        } catch (err) {
          console.error("new terminal erro:", err);
        }
        return;
      }
      if (e.key === "k" || e.key === "K") {
        e.preventDefault();
        e.stopPropagation();
        getPaneActions(term.activePane)?.clear();
        return;
      }
      // Font zoom (Cmd+=, Cmd++, Cmd+-, Cmd+0).
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
    }

    // Ctrl+letter (no Cmd, no Alt): send the control byte straight to the active PTY.
    // We use e.code (physical, layout-independent) because e.key may come empty/Dead
    // depending on WebKit/keyboard.
    if (e.ctrlKey && !e.metaKey && !e.altKey && term) {
      const m = e.code.match(/^Key([A-Z])$/);
      if (m) {
        const code = m[1].charCodeAt(0) - 64; // Ctrl+A=1, Ctrl+R=18, Ctrl+Z=26
        e.preventDefault();
        e.stopPropagation();
        api
          .writeInput(term.activePane, String.fromCharCode(code))
          .catch((err) => console.error("ctrl input erro:", err));
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
</script>

<main class="app">
  <Sidebar
    onRequestClose={(id) =>
      (pending = { kind: "terminal", terminalId: id })}
    onOpenPreferences={() => (prefsOpen = true)}
  />
  <section class="content">
    <div class="pane-host">
      {#each $terminals as t (t.id)}
        <div class="pane" class:active={t.id === $activeId}>
          <PaneTreeView
            node={t.layout}
            terminal={t}
            terminalActive={t.id === $activeId}
          />
        </div>
      {/each}
    </div>
    <StatusBar />
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

<PreferencesDialog open={prefsOpen} onClose={() => (prefsOpen = false)} />
