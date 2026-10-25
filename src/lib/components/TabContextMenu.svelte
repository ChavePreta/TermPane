<script lang="ts" module>
  export type MenuItem = {
    label: string;
    hint?: string;
    danger?: boolean;
    // Draws a separator line above this item.
    dividerBefore?: boolean;
    // Starts a titled group (e.g. "Claude") above this item.
    section?: string;
    run: () => void;
  };
</script>

<script lang="ts">
  type Item = MenuItem;

  type Props = {
    x: number;
    y: number;
    items: Item[];
    onClose: () => void;
  };

  let { x, y, items, onClose }: Props = $props();

  let menuEl: HTMLDivElement | null = $state(null);

  // Keep the menu inside the window.
  let left = $derived(Math.min(x, window.innerWidth - 220));
  let top = $derived(Math.min(y, window.innerHeight - items.length * 32 - 16));

  function pick(item: Item) {
    onClose();
    item.run();
  }

  function handlePointerDown(e: PointerEvent) {
    if (menuEl && e.target instanceof Node && !menuEl.contains(e.target)) onClose();
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  }

  $effect(() => {
    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => document.removeEventListener("pointerdown", handlePointerDown, true);
  });
</script>

<svelte:window onkeydown={handleKey} onblur={onClose} />

<div class="menu" role="menu" bind:this={menuEl} style="left: {left}px; top: {top}px">
  {#each items as item}
    {#if item.dividerBefore}<div class="divider" role="separator"></div>{/if}
    {#if item.section}<div class="section">{item.section}</div>{/if}
    <button class="item" class:danger={item.danger} role="menuitem" onclick={() => pick(item)}>
      <span>{item.label}</span>
      {#if item.hint}<kbd>{item.hint}</kbd>{/if}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 900;
    min-width: 200px;
    padding: 4px;
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
    -webkit-app-region: no-drag;
  }

  .item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 6px 9px;
    border-radius: 5px;
    color: var(--fg);
    text-align: left;
    font-size: 13px;
  }

  .item:hover {
    background: var(--bg-hover);
  }

  .item.danger:hover {
    background: var(--danger);
    color: #fff;
  }

  .section {
    padding: 6px 9px 2px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--fg-faint);
  }

  .divider {
    height: 1px;
    margin: 4px 2px;
    background: var(--border);
  }

  kbd {
    font-family: inherit;
    font-size: 11px;
    color: var(--fg-muted);
  }
</style>
