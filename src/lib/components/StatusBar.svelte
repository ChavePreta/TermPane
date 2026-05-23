<script lang="ts">
  import { writable } from "svelte/store";
  import { browser } from "$app/environment";
  import { activeTerminal } from "../store";
  import { broadcastEnabled, toggleBroadcast } from "../broadcast";
  import { platform, shortcut } from "../platform";

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

  type ShortcutSpec = { mac: string; linux: string; label: string };
  const shortcuts: ShortcutSpec[] = [
    { mac: "⌘T", linux: "Ctrl+Shift+T", label: "new terminal" },
    { mac: "⌘D", linux: "Ctrl+Shift+D", label: "split vertical" },
    { mac: "⌘⇧D", linux: "Ctrl+Shift+Alt+D", label: "split horizontal" },
    { mac: "⌘/", linux: "Ctrl+Shift+/", label: "flip split" },
    { mac: "⌘⇧E", linux: "Ctrl+Shift+Alt+E", label: "extract pane" },
    { mac: "⌘K", linux: "Ctrl+Shift+K", label: "clear" },
    { mac: "⌘W", linux: "Ctrl+Shift+W", label: "close pane" },
    { mac: "⌘B", linux: "Ctrl+Shift+B", label: "toggle sidebar" },
  ];

  function toggle() {
    visible.update((v) => !v);
  }

  let canBroadcast = $derived(($activeTerminal?.panes.length ?? 0) >= 2);
  let isBroadcasting = $derived(
    $activeTerminal ? $broadcastEnabled.has($activeTerminal.id) : false,
  );
</script>

{#if $visible}
  <footer class="status-bar">
    <div class="hints">
      {#each shortcuts as s}
        <span class="hint">
          <kbd>{shortcut($platform, s.mac, s.linux)}</kbd>
          <span class="hint-label">{s.label}</span>
        </span>
      {/each}
    </div>
    <div class="actions">
      {#if canBroadcast}
        <button
          class="broadcast-btn"
          class:on={isBroadcasting}
          title={isBroadcasting
            ? "Broadcast on — typing mirrored to all panes"
            : "Broadcast typing to all panes of this terminal"}
          aria-pressed={isBroadcasting}
          onclick={() => $activeTerminal && toggleBroadcast($activeTerminal.id)}
        >
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M2 12a10 10 0 0 1 20 0" />
            <path d="M6 12a6 6 0 0 1 12 0" />
            <circle cx="12" cy="12" r="2" />
          </svg>
          <span>Broadcast{isBroadcasting ? ": on" : ""}</span>
        </button>
      {/if}
      <button class="toggle" onclick={toggle} title="Hide shortcut bar" aria-label="Hide shortcut bar">
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
    </div>
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

  .actions {
    display: inline-flex;
    align-items: center;
    gap: 8px;
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

  .hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .hint-label {
    color: var(--fg-muted);
  }

  .broadcast-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--fg-muted);
    border: 1px solid var(--border);
    background: var(--bg-elev-1);
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }

  .broadcast-btn:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .broadcast-btn.on {
    color: var(--bg);
    background: var(--danger);
    border-color: var(--danger);
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
