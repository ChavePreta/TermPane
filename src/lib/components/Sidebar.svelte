<script lang="ts">
  import { terminals, activeId } from "../store";
  import { api } from "../api";
  import TerminalCard from "./TerminalCard.svelte";
  import ThemeSwitcher from "./ThemeSwitcher.svelte";
  import PinButton from "./PinButton.svelte";
  import LinkButtons from "./LinkButtons.svelte";

  type Props = {
    onRequestClose: (id: string) => void;
    onOpenPreferences: () => void;
  };

  let { onRequestClose, onOpenPreferences }: Props = $props();

  let listEl: HTMLDivElement | null = $state(null);

  // Drag state
  let dragId = $state<string | null>(null);
  let dragStarted = $state(false);
  let dropIndex = $state<number | null>(null);
  let dragStartX = 0;
  let dragStartY = 0;
  let originalIndex = 0;
  const DRAG_THRESHOLD = 5;

  async function handleNew() {
    try {
      const snap = await api.openTerminal();
      activeId.set(snap.id);
    } catch (e) {
      console.error("openTerminal falhou:", e);
    }
  }

  async function handleSelect(id: string) {
    activeId.set(id);
    try {
      await api.setActiveTerminal(id);
    } catch (e) {
      console.error("setActiveTerminal falhou:", e);
    }
  }

  function handleListPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (!target) return;
    // Ignore pointerdown on input (edit mode) and on the close button.
    if (target.closest(".card-edit")) return;
    if (target.closest(".card-close")) return;
    const card = target.closest<HTMLElement>(".card[data-card-id]");
    if (!card) return;
    const id = card.dataset.cardId!;
    const list = $terminals;
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
    dragId = null;
    dragStarted = false;
    dropIndex = null;
    if (!wasDragging || draggedId === null || targetIdx === null) return;

    const list = $terminals;
    const orig = list.findIndex((t) => t.id === draggedId);
    if (orig < 0) return;
    // Normalize targetIdx: removing before the target position shifts it left.
    let dest = targetIdx;
    if (orig < dest) dest -= 1;
    if (dest === orig) return;

    const newIds = list.map((t) => t.id);
    const [moved] = newIds.splice(orig, 1);
    newIds.splice(dest, 0, moved);
    // Aplica otimisticamente; backend confirma via terminals:reordered.
    terminals.reorder(newIds);
    try {
      await api.reorderTerminals(newIds);
    } catch (e) {
      console.error("reorderTerminals:", e);
    }
  }
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <span>TermPane</span>
    <div class="sidebar-tools">
      <PinButton />
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
    </div>
  </div>
  <div
    class="sidebar-list"
    bind:this={listEl}
    onpointerdown={handleListPointerDown}
    role="list"
  >
    {#each $terminals as t, i (t.id)}
      {#if dragStarted && dropIndex === i}
        <div class="drop-indicator"></div>
      {/if}
      <TerminalCard
        terminal={t}
        active={t.id === $activeId}
        dragging={dragId === t.id && dragStarted}
        onSelect={() => handleSelect(t.id)}
        onRequestClose={() => onRequestClose(t.id)}
      />
    {/each}
    {#if dragStarted && dropIndex !== null && dropIndex >= $terminals.length}
      <div class="drop-indicator"></div>
    {/if}
  </div>
  <div class="sidebar-footer">
    <button class="new-terminal-btn" onclick={handleNew}>+ New terminal</button>
    <LinkButtons />
  </div>
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
</style>
