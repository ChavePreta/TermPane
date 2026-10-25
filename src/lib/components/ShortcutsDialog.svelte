<script lang="ts">
  import { shortcuts, saveShortcuts, type Shortcuts } from "../shortcuts";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open, onClose }: Props = $props();

  type Row = {
    id: string;
    name: string;
    cwd: string;
    command: string;
    shell: string;
    expanded: boolean;
  };

  let rows = $state<Row[]>([]);
  let saving = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) {
      rows = $shortcuts.items.map((s) => ({
        id: s.id,
        name: s.name,
        cwd: s.cwd ?? "",
        command: s.command ?? "",
        shell: s.shell ?? "",
        expanded: false,
      }));
      error = null;
    }
  });

  function addRow() {
    rows = [
      ...rows,
      {
        id: crypto.randomUUID(),
        name: "",
        cwd: "",
        command: "",
        shell: "",
        expanded: true,
      },
    ];
  }

  function removeRow(id: string) {
    rows = rows.filter((r) => r.id !== id);
  }

  function moveRow(index: number, delta: number) {
    const j = index + delta;
    if (j < 0 || j >= rows.length) return;
    const next = [...rows];
    [next[index], next[j]] = [next[j], next[index]];
    rows = next;
  }

  async function handleSave() {
    saving = true;
    error = null;
    const payload: Shortcuts = {
      schemaVersion: 1,
      items: rows.map((r) => ({
        id: r.id,
        name: r.name.trim(),
        cwd: r.cwd.trim() || null,
        command: r.command.trim() || null,
        shell: r.shell.trim() || null,
      })),
    };
    try {
      await saveShortcuts(payload);
      onClose();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKey} />

{#if open}
  <div
    class="backdrop"
    onclick={onClose}
    onkeydown={(e) => e.key === "Escape" && onClose()}
    role="presentation"
  >
    <div
      class="dialog"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="shortcuts-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 id="shortcuts-title">Custom terminals</h2>
      <p class="hint">
        Named shortcuts to open a terminal in a directory and/or run a command.
      </p>

      <div class="list">
        {#each rows as row, i (row.id)}
          <div class="shortcut" class:expanded={row.expanded}>
            <div class="shortcut-header">
              <button
                class="shortcut-toggle"
                aria-expanded={row.expanded}
                onclick={() => (row.expanded = !row.expanded)}
              >
                <span class="title" class:untitled={!row.name.trim()}>
                  {row.name.trim() || "Untitled"}
                </span>
              </button>
              <button
                type="button"
                class="move"
                aria-label="Move up"
                title="Move up"
                disabled={i === 0}
                onclick={() => moveRow(i, -1)}
              >
                ↑
              </button>
              <button
                type="button"
                class="move"
                aria-label="Move down"
                title="Move down"
                disabled={i === rows.length - 1}
                onclick={() => moveRow(i, 1)}
              >
                ↓
              </button>
              <button
                type="button"
                class="toggle-btn"
                aria-label={row.expanded ? "Collapse" : "Expand"}
                onclick={() => (row.expanded = !row.expanded)}
              >
                <span class="toggle" aria-hidden="true">
                  {row.expanded ? "−" : "+"}
                </span>
              </button>
            </div>

            {#if row.expanded}
              <div class="shortcut-body">
                <input
                  class="name"
                  type="text"
                  placeholder="Name"
                  bind:value={row.name}
                />
                <input
                  type="text"
                  placeholder="Working directory (optional) — e.g. ~/Projetos/app"
                  bind:value={row.cwd}
                />
                <input
                  type="text"
                  placeholder="Startup command (optional) — e.g. claude"
                  bind:value={row.command}
                />
                <input
                  type="text"
                  placeholder="Shell (optional) — e.g. /bin/bash"
                  bind:value={row.shell}
                />
                <button class="remove" onclick={() => removeRow(row.id)}>
                  Remove
                </button>
              </div>
            {/if}
          </div>
        {/each}

        {#if rows.length === 0}
          <p class="empty">No shortcuts yet. Add one below.</p>
        {/if}
      </div>

      <button class="add" onclick={addRow}>+ Add shortcut</button>

      {#if error}
        <p class="error">{error}</p>
      {/if}

      <div class="actions">
        <button class="btn-secondary" onclick={onClose} disabled={saving}>
          Cancel
        </button>
        <button class="btn-primary" onclick={handleSave} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
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
    padding: 22px 24px 18px 24px;
    min-width: 460px;
    max-width: 560px;
    color: var(--fg);
  }

  h2 {
    margin: 0 0 4px 0;
    font-size: 15px;
  }

  .hint {
    margin: 0 0 14px 0;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 50vh;
    overflow-y: auto;
    margin-bottom: 12px;
  }

  .shortcut {
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }

  .shortcut.expanded {
    background: var(--bg);
  }

  .shortcut-header {
    display: flex;
    align-items: stretch;
  }

  .shortcut-toggle {
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 0;
    padding: 9px 4px 9px 11px;
    color: var(--fg);
    text-align: left;
    font-size: 13px;
    transition: background 0.12s;
  }

  .shortcut-toggle:hover {
    background: var(--bg-hover);
  }

  .move,
  .toggle-btn {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-muted);
    transition: background 0.12s, color 0.12s;
  }

  .move {
    width: 22px;
    font-size: 13px;
    line-height: 1;
    border-radius: 4px;
    margin: 4px 0;
  }

  .move:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .toggle-btn {
    padding: 0 9px 0 5px;
  }

  .toggle-btn:hover {
    color: var(--fg);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title.untitled {
    color: var(--fg-faint);
    font-style: italic;
  }

  .toggle {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 15px;
    line-height: 1;
    color: var(--fg-muted);
  }

  .shortcut-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 11px 11px 11px;
  }

  input[type="text"] {
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 7px 9px;
    color: var(--fg);
    font-family: inherit;
    font-size: 13px;
    outline: none;
    transition: border-color 0.12s;
  }

  input[type="text"]:focus {
    border-color: var(--accent);
  }

  .name {
    font-weight: 500;
  }

  .remove {
    align-self: flex-start;
    margin-top: 2px;
    padding: 4px 2px;
    color: var(--fg-muted);
    font-size: 12px;
    transition: color 0.12s;
  }

  .remove:hover {
    color: var(--danger);
  }

  .empty {
    margin: 0;
    padding: 8px 2px;
    font-size: 12px;
    color: var(--fg-faint);
  }

  .add {
    width: 100%;
    padding: 8px 10px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    color: var(--fg-muted);
    font-size: 13px;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }

  .add:hover {
    background: var(--bg-hover);
    color: var(--fg);
    border-color: var(--accent);
  }

  .error {
    margin: 12px 0 0 0;
    color: var(--danger);
    font-size: 12px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }

  .btn-secondary,
  .btn-primary {
    padding: 7px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    transition: background 0.12s, opacity 0.12s;
  }

  .btn-secondary {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--bg-active);
  }

  .btn-primary {
    background: var(--accent);
    color: #fff;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.08);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
