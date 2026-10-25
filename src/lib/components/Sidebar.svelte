<script lang="ts">
  import {
    terminals,
    activeId,
    selectTerminal,
    terminalDepths,
    subtreeIds,
  } from "../store";
  import type { TerminalSnapshot } from "../api";
  import { api } from "../api";
  import { dropTargetPaneId } from "../dragState";
  import { platform, shortcut } from "../platform";
  import TerminalCard from "./TerminalCard.svelte";
  import ThemeSwitcher from "./ThemeSwitcher.svelte";
  import PinButton from "./PinButton.svelte";
  import LinkButtons from "./LinkButtons.svelte";
  import CustomTerminalMenu from "./CustomTerminalMenu.svelte";
  import TabContextMenu from "./TabContextMenu.svelte";
  import {
    sidebarCollapsed,
    sidebarWidth,
    collapsedTabs,
    toggleCollapsedTab,
  } from "../sidebarLayout";

  type Props = {
    onRequestClose: (id: string) => void;
    onOpenPreferences: () => void;
    onManageShortcuts: () => void;
    onOpenFiles: (terminalId: string) => void;
    onOpenResume: () => void;
  };

  let {
    onRequestClose,
    onOpenPreferences,
    onManageShortcuts,
    onOpenFiles,
    onOpenResume,
  }: Props = $props();

  let depths = $derived(terminalDepths($terminals));
  let parentIds = $derived(
    new Set($terminals.map((t) => t.parentId).filter((p): p is string => !!p)),
  );
  // Cards shown in the sidebar: everything except descendants of a collapsed tab.
  let visible = $derived.by(() => {
    const hidden = new Set<string>();
    for (const t of $terminals) {
      if (t.parentId && (hidden.has(t.parentId) || $collapsedTabs.has(t.parentId))) {
        hidden.add(t.id);
      }
    }
    return $terminals.filter((t) => !hidden.has(t.id));
  });

  let menu = $state<{ x: number; y: number; terminal: TerminalSnapshot } | null>(null);

  const mod = (mac: string, linux: string) => shortcut($platform, mac, linux);

  type MenuItem = { label: string; hint?: string; danger?: boolean; run: () => void };

  let menuItems = $derived.by((): MenuItem[] => {
    if (!menu) return [];
    const t = menu.terminal;
    const items: MenuItem[] = [
      {
        label: "New sub-tab",
        hint: mod("⌘⇧T", "Ctrl+Shift+Alt+T"),
        run: () => openSubTab(t.id),
      },
      {
        label: t.markedUnread ? "Mark as read" : "Mark as unread",
        hint: mod("⌘⇧U", "Ctrl+Shift+Alt+U"),
        run: () =>
          api.setTerminalUnread(t.id, !t.markedUnread).catch((e) =>
            console.error("setTerminalUnread:", e),
          ),
      },
      { label: "Files changed by Claude…", run: () => onOpenFiles(t.id) },
    ];
    const devPane =
      t.panes.find((p) => p.id === t.activePane && p.devContainer) ?? t.panes.find((p) => p.devContainer);
    if (devPane) {
      items.push({
        label: `Open shell in dev container (${devPane.devContainer})`,
        run: () =>
          api
            .openDevcontainerShell(devPane.id)
            .then((snap) => selectTerminal(snap.id))
            .catch((e) => console.error("dev container shell:", e)),
      });
    }
    if (t.parentId) {
      items.push({
        label: "Move to top level",
        run: () =>
          api
            .moveTerminal(t.id, null, $terminals.map((x) => x.id))
            .catch((e) => console.error("moveTerminal:", e)),
      });
    }
    items.push({ label: "Close", danger: true, run: () => onRequestClose(t.id) });
    return items;
  });

  async function openSubTab(parentId: string) {
    try {
      const snap = await api.openTerminal({ parentId });
      collapsedTabs.update((s) => {
        if (!s.has(parentId)) return s;
        const next = new Set(s);
        next.delete(parentId);
        return next;
      });
      await selectTerminal(snap.id);
    } catch (e) {
      console.error("open sub-tab:", e);
    }
  }

  let listEl: HTMLDivElement | null = $state(null);

  // Resize handle state
  let resizing = $state(false);
  let resizeStartX = 0;
  let resizeStartW = 0;

  function handleResizePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    resizing = true;
    resizeStartX = e.clientX;
    resizeStartW = $sidebarWidth;
    (e.target as Element).setPointerCapture?.(e.pointerId);
    window.addEventListener("pointermove", handleResizeMove);
    window.addEventListener("pointerup", handleResizeUp);
  }

  function handleResizeMove(e: PointerEvent) {
    if (!resizing) return;
    sidebarWidth.set(resizeStartW + (e.clientX - resizeStartX));
  }

  function handleResizeUp() {
    resizing = false;
    window.removeEventListener("pointermove", handleResizeMove);
    window.removeEventListener("pointerup", handleResizeUp);
  }

  // Drag state
  let dragId = $state<string | null>(null);
  let dragStarted = $state(false);
  let dropIndex = $state<number | null>(null);
  let dropPaneId: string | null = null;
  let lastShift = false;
  let dragStartX = 0;
  let dragStartY = 0;
  let originalIndex = 0;
  const DRAG_THRESHOLD = 5;

  function paneIdUnderPointer(clientX: number, clientY: number): string | null {
    let el: Element | null = document.elementFromPoint(clientX, clientY);
    while (el) {
      if (el instanceof HTMLElement && el.dataset.paneId) return el.dataset.paneId;
      el = el.parentElement;
    }
    return null;
  }

  async function handleNew() {
    try {
      const snap = await api.openTerminal();
      activeId.set(snap.id);
    } catch (e) {
      console.error("openTerminal falhou:", e);
    }
  }

  function handleListPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (!target) return;
    // Ignore pointerdown on input (edit mode) and on the close button.
    if (target.closest(".card-edit")) return;
    if (target.closest(".card-close")) return;
    if (target.closest(".card-toggle, .card-files, .card-info")) return;
    const card = target.closest<HTMLElement>(".card[data-card-id]");
    if (!card) return;
    const id = card.dataset.cardId!;
    const list = visible;
    const idx = list.findIndex((t) => t.id === id);
    if (idx < 0) return;
    // Prevents the browser from starting text selection while dragging over cards.
    e.preventDefault();
    dragId = id;
    dragStarted = false;
    dropIndex = idx;
    originalIndex = idx;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", handleUp);
  }

  function handleMove(e: PointerEvent) {
    if (!dragId) return;
    if (!dragStarted) {
      const dx = e.clientX - dragStartX;
      const dy = e.clientY - dragStartY;
      if (dx * dx + dy * dy < DRAG_THRESHOLD * DRAG_THRESHOLD) return;
      dragStarted = true;
      document.body.classList.add("termpane-dragging");
    }
    lastShift = e.shiftKey;
    const pane = paneIdUnderPointer(e.clientX, e.clientY);
    if (pane) {
      // Cards belonging to the dragged terminal would be inside its own pane area
      // (none right now — sidebar cards are outside .pane). But the backend also
      // refuses self-merge, so we only suppress here to avoid the highlight.
      const term = $terminals.find((t) =>
        t.panes.some((p) => p.id === pane),
      );
      if (term && term.id !== dragId) {
        dropPaneId = pane;
        dropTargetPaneId.set(pane);
        dropIndex = null;
        return;
      }
    }
    dropPaneId = null;
    dropTargetPaneId.set(null);
    dropIndex = computeDropIndex(e.clientY);
  }

  function computeDropIndex(clientY: number): number {
    if (!listEl) return originalIndex;
    const cards = Array.from(
      listEl.querySelectorAll<HTMLElement>(".card[data-card-id]"),
    );
    for (let i = 0; i < cards.length; i++) {
      const r = cards[i].getBoundingClientRect();
      const mid = r.top + r.height / 2;
      if (clientY < mid) return i;
    }
    return cards.length;
  }

  async function handleUp() {
    window.removeEventListener("pointermove", handleMove);
    window.removeEventListener("pointerup", handleUp);
    document.body.classList.remove("termpane-dragging");
    const wasDragging = dragStarted;
    const draggedId = dragId;
    const targetIdx = dropIndex;
    const droppedOnPane = dropPaneId;
    const shift = lastShift;
    dragId = null;
    dragStarted = false;
    dropIndex = null;
    dropPaneId = null;
    dropTargetPaneId.set(null);
    if (!wasDragging || draggedId === null) return;

    if (droppedOnPane) {
      try {
        const snap = await api.mergeTerminalIntoPane(
          draggedId,
          droppedOnPane,
          shift ? "vertical" : "horizontal",
        );
        activeId.set(snap.id);
      } catch (e) {
        console.error("mergeTerminalIntoPane:", e);
      }
      return;
    }

    if (targetIdx === null) return;

    // The dragged card moves with its whole subtree. It lands before the card
    // under the drop indicator and takes that card's parent — so dropping
    // between a tab and its first sub-tab nests it, and dropping between two
    // top-level tabs (or at the end) makes it top level.
    const all = $terminals;
    const moving = new Set(subtreeIds(all, draggedId));
    const below = visible.slice(targetIdx).find((t) => !moving.has(t.id)) ?? null;
    const newParent = below?.parentId ?? null;
    const order = all.filter((t) => !moving.has(t.id)).map((t) => t.id);
    const block = all.filter((t) => moving.has(t.id)).map((t) => t.id);
    order.splice(below ? order.indexOf(below.id) : order.length, 0, ...block);
    const oldParent = all.find((t) => t.id === draggedId)?.parentId ?? null;
    if (newParent === oldParent && order.every((id, i) => all[i].id === id)) return;
    try {
      await api.moveTerminal(draggedId, newParent, order);
    } catch (e) {
      console.error("moveTerminal:", e);
    }
  }
</script>

<aside class="sidebar" style="width: {$sidebarWidth}px">
  <div class="sidebar-header">
    <span>TermPane</span>
    <div class="sidebar-tools">
      <PinButton />
      <button
        class="icon-btn"
        title={`Resume a Claude session (${mod("⌘⇧O", "Ctrl+Shift+Alt+O")})`}
        aria-label="Resume a Claude session"
        onclick={onOpenResume}
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M3 3v5h5" />
          <path d="M3.05 13A9 9 0 1 0 6 5.3L3 8" />
          <path d="M12 7v5l3 2" />
        </svg>
      </button>
      <button
        class="icon-btn"
        title="Preferences"
        aria-label="Preferences"
        onclick={onOpenPreferences}
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      </button>
      <ThemeSwitcher />
      <button
        class="icon-btn"
        title={`Hide sidebar (${shortcut($platform, "⌘B", "Ctrl+Shift+B")})`}
        aria-label="Hide sidebar"
        onclick={() => sidebarCollapsed.set(true)}
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </button>
    </div>
  </div>
  <div
    class="sidebar-list"
    bind:this={listEl}
    onpointerdown={handleListPointerDown}
    role="list"
  >
    {#each visible as t, i (t.id)}
      {#if dragStarted && dropIndex === i}
        <div class="drop-indicator" style="margin-left: {4 + (depths.get(t.id) ?? 0) * 16}px"></div>
      {/if}
      <TerminalCard
        terminal={t}
        active={t.id === $activeId}
        dragging={dragId === t.id && dragStarted}
        depth={depths.get(t.id) ?? 0}
        hasChildren={parentIds.has(t.id)}
        collapsed={$collapsedTabs.has(t.id)}
        onToggleCollapse={() => toggleCollapsedTab(t.id)}
        onSelect={() => selectTerminal(t.id)}
        onRequestClose={() => onRequestClose(t.id)}
        onContextMenu={(e) => (menu = { x: e.clientX, y: e.clientY, terminal: t })}
        onOpenFiles={() => onOpenFiles(t.id)}
      />
    {/each}
    {#if dragStarted && dropIndex !== null && dropIndex >= visible.length}
      <div class="drop-indicator"></div>
    {/if}
  </div>
  <div class="sidebar-footer">
    <div class="footer-actions">
      <button class="new-terminal-btn" onclick={handleNew}>+ New terminal</button>
      <CustomTerminalMenu onManage={onManageShortcuts} />
    </div>
    <LinkButtons />
  </div>
  {#if menu}
    <TabContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => (menu = null)} />
  {/if}
  <div
    class="sidebar-resizer"
    class:active={resizing}
    onpointerdown={handleResizePointerDown}
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
  ></div>
</aside>

<style>
  .sidebar-tools {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .icon-btn {
    -webkit-app-region: no-drag;
    width: 24px;
    height: 24px;
    border-radius: 5px;
    color: var(--fg-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s, color 0.12s;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .drop-indicator {
    height: 2px;
    background: var(--accent);
    border-radius: 1px;
    margin: 1px 4px;
    pointer-events: none;
  }

  .sidebar-resizer {
    position: absolute;
    top: 0;
    right: -2px;
    width: 4px;
    height: 100%;
    cursor: ew-resize;
    background: transparent;
    transition: background 0.12s;
    z-index: 20;
    -webkit-app-region: no-drag;
  }

  .sidebar-resizer:hover,
  .sidebar-resizer.active {
    background: var(--accent);
  }
</style>
