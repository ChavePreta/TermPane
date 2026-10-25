<script lang="ts">
  import { shortcutGroups } from "../keyboardShortcuts";
  import { platform } from "../platform";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open, onClose }: Props = $props();

  let isMac = $derived($platform === "mac");

  function handleKey(e: KeyboardEvent) {
    if (open && e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKey} />

{#if open}
  <div class="backdrop" onclick={onClose} role="presentation">
    <div
      class="dialog"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="kbd-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 id="kbd-title">Keyboard shortcuts</h2>
      <div class="groups">
        {#each shortcutGroups as group (group.title)}
          <section>
            <h3>{group.title}</h3>
            <dl>
              {#each group.items as item (item.label)}
                <div class="row">
                  <dt>{item.label}</dt>
                  <dd><kbd>{isMac ? item.mac : item.linux}</kbd></dd>
                </div>
              {/each}
            </dl>
          </section>
        {/each}
      </div>
      <p class="foot">
        Any Ctrl+letter (Ctrl+C, Ctrl+R, Ctrl+A, …) goes straight to the terminal.
      </p>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    padding: 22px 24px 16px 24px;
    width: min(760px, calc(100vw - 48px));
    max-height: calc(100vh - 60px);
    overflow-y: auto;
    color: var(--fg);
  }

  h2 {
    margin: 0 0 14px 0;
    font-size: 15px;
  }

  .groups {
    columns: 2 300px;
    column-gap: 28px;
  }

  section {
    break-inside: avoid;
    margin-bottom: 16px;
  }

  h3 {
    margin: 0 0 6px 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }

  dl {
    margin: 0;
  }

  .row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  }

  dt {
    font-size: 12.5px;
    min-width: 0;
  }

  dd {
    margin: 0;
    flex-shrink: 0;
  }

  kbd {
    font-family: inherit;
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--bg-hover);
    color: var(--fg);
    border: 1px solid var(--border);
    white-space: nowrap;
  }

  .foot {
    margin: 4px 0 0 0;
    font-size: 11px;
    color: var(--fg-faint);
  }
</style>
