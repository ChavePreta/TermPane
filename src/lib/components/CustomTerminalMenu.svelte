<script lang="ts">
  import { api } from "../api";
  import { activeId } from "../store";
  import { shortcuts, type CustomShortcut } from "../shortcuts";

  type Props = {
    onManage: () => void;
  };

  let { onManage }: Props = $props();

  let open = $state(false);
  let wrapperEl: HTMLDivElement | null = $state(null);

  function toggle() {
    open = !open;
  }

  async function launch(s: CustomShortcut) {
    open = false;
    try {
      const snap = await api.openTerminal({
        cwd: s.cwd,
        command: s.command,
        shell: s.shell,
        name: s.name,
      });
      activeId.set(snap.id);
    } catch (e) {
      console.error("openTerminal (custom) failed:", e);
    }
  }

  function manage() {
    open = false;
    onManage();
  }

  function handlePointerDown(e: PointerEvent) {
    if (wrapperEl && e.target instanceof Node && !wrapperEl.contains(e.target)) {
      open = false;
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (open && e.key === "Escape") {
      e.preventDefault();
      open = false;
    }
  }

  $effect(() => {
    if (open) {
      document.addEventListener("pointerdown", handlePointerDown, true);
      return () =>
        document.removeEventListener("pointerdown", handlePointerDown, true);
    }
  });
</script>

<svelte:window onkeydown={handleKey} />

<div class="custom-menu" bind:this={wrapperEl}>
  {#if open}
    <div class="dropdown" role="menu">
      {#if $shortcuts.items.length === 0}
        <div class="empty">No shortcuts yet</div>
      {:else}
        {#each $shortcuts.items as s (s.id)}
          <button
            class="item"
            role="menuitem"
            title={s.command ?? s.cwd ?? s.name}
            onclick={() => launch(s)}
          >
            {s.name}
          </button>
        {/each}
      {/if}
      <div class="separator"></div>
      <button class="item manage" role="menuitem" onclick={manage}>
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
        Manage shortcuts…
      </button>
    </div>
  {/if}
  <button
    class="custom-btn"
    class:active={open}
    title="Custom terminal shortcuts"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={toggle}
  >
    + custom <span class="caret">▾</span>
  </button>
</div>

<style>
  .custom-menu {
    position: relative;
    -webkit-app-region: no-drag;
  }

  .custom-btn {
    padding: 8px 10px;
    border-radius: 6px;
    color: var(--fg-muted);
    white-space: nowrap;
    transition: background 0.12s, color 0.12s;
  }

  .custom-btn:hover,
  .custom-btn.active {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .caret {
    font-size: 9px;
    opacity: 0.8;
  }

  .dropdown {
    position: absolute;
    bottom: calc(100% + 4px);
    right: 0;
    -webkit-app-region: no-drag;
    min-width: 180px;
    max-width: 280px;
    max-height: 320px;
    overflow-y: auto;
    padding: 4px;
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
    z-index: 50;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 7px 9px;
    border-radius: 5px;
    color: var(--fg);
    text-align: left;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: background 0.12s;
  }

  .item:hover {
    background: var(--bg-hover);
  }

  .item.manage {
    color: var(--fg-muted);
  }

  .item.manage:hover {
    color: var(--fg);
  }

  .empty {
    padding: 7px 9px;
    color: var(--fg-faint);
    font-size: 12px;
  }

  .separator {
    height: 1px;
    background: var(--border);
    margin: 4px 2px;
  }
</style>
