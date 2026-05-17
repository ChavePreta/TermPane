<script lang="ts">
  import { writable } from "svelte/store";
  import { browser } from "$app/environment";

  const STORAGE_KEY = "termpane.statusbar.visible";

  function initial(): boolean {
    if (!browser) return true;
    const v = localStorage.getItem(STORAGE_KEY);
    return v === null ? true : v === "1";
  }

  const visible = writable<boolean>(initial());

  if (browser) {
    visible.subscribe((v) => {
      try {
        localStorage.setItem(STORAGE_KEY, v ? "1" : "0");
      } catch {}
    });
  }

  const shortcuts: { keys: string; label: string }[] = [
    { keys: "⌘T", label: "new terminal" },
    { keys: "⌘D", label: "split vertical" },
    { keys: "⌘⇧D", label: "split horizontal" },
    { keys: "⌘⇧E", label: "extract pane" },
    { keys: "⌘K", label: "clear" },
    { keys: "⌘W", label: "close pane" },
    { keys: "⌘B", label: "toggle sidebar" },
  ];

  function toggle() {
    visible.update((v) => !v);
  }
</script>

{#if $visible}
  <footer class="status-bar">
    <div class="hints">
      {#each shortcuts as s}
        <span class="hint">
          <kbd>{s.keys}</kbd>
          <span class="hint-label">{s.label}</span>
        </span>
      {/each}
    </div>
    <button class="toggle" onclick={toggle} title="Hide shortcut bar" aria-label="Hide shortcut bar">
      <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>
  </footer>
{:else}
  <button class="reveal" onclick={toggle} title="Show shortcut bar" aria-label="Show shortcut bar">
    <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <polyline points="18 15 12 9 6 15" />
    </svg>
  </button>
{/if}

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px 4px 12px;
    border-top: 1px solid var(--border);
    background: var(--bg-elev-1);
    color: var(--fg-muted);
    font-size: 11px;
    flex-shrink: 0;
    user-select: none;
  }

  .hints {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }

  .hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  kbd {
    font-family: inherit;
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--bg-hover);
    color: var(--fg);
    border: 1px solid var(--border);
    font-variant-numeric: tabular-nums;
  }

  .hint-label {
    color: var(--fg-muted);
  }

  .toggle,
  .reveal {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: var(--fg-muted);
    transition: background 0.12s, color 0.12s;
  }

  .toggle:hover,
  .reveal:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .reveal {
    position: absolute;
    bottom: 6px;
    right: 8px;
    z-index: 10;
    background: var(--bg-elev-1);
    border: 1px solid var(--border);
  }
</style>
