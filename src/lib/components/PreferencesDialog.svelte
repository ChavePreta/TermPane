<script lang="ts">
  import { preferences, savePreferences, type Preferences } from "../preferences";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open, onClose }: Props = $props();

  let fontFamily = $state("");
  let fontSize = $state(13);
  let lineHeight = $state(1.15);
  let cursorBlink = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) {
      const p = $preferences;
      fontFamily = p.fontFamily;
      fontSize = p.fontSize;
      lineHeight = p.lineHeight;
      cursorBlink = p.cursorBlink;
      error = null;
    }
  });

  async function handleSave() {
    saving = true;
    error = null;
    const next: Preferences = {
      ...$preferences,
      fontFamily: fontFamily.trim() || $preferences.fontFamily,
      fontSize: clamp(Math.round(fontSize), 8, 32),
      lineHeight: clamp(lineHeight, 1.0, 2.0),
      cursorBlink,
    };
    try {
      await savePreferences(next);
      onClose();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  function clamp(v: number, lo: number, hi: number) {
    if (!Number.isFinite(v)) return lo;
    return Math.max(lo, Math.min(hi, v));
  }

  function resetDefaults() {
    fontFamily =
      'Menlo, Monaco, "JetBrains Mono", "Fira Code", Consolas, monospace';
    fontSize = 13;
    lineHeight = 1.15;
    cursorBlink = true;
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
      aria-labelledby="prefs-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 id="prefs-title">Preferences</h2>

      <div class="field">
        <label for="ff">Font family</label>
        <input
          id="ff"
          type="text"
          bind:value={fontFamily}
          placeholder="Menlo, Monaco, monospace"
        />
        <small>CSS list — first available font wins.</small>
      </div>

      <div class="row">
        <div class="field">
          <label for="fs">Size ({fontSize}px)</label>
          <input
            id="fs"
            type="range"
            min="8"
            max="32"
            step="1"
            bind:value={fontSize}
          />
        </div>
        <div class="field">
          <label for="lh">Line height ({lineHeight.toFixed(2)})</label>
          <input
            id="lh"
            type="range"
            min="1"
            max="2"
            step="0.05"
            bind:value={lineHeight}
          />
        </div>
      </div>

      <div class="field checkbox">
        <label>
          <input type="checkbox" bind:checked={cursorBlink} />
          Blinking cursor
        </label>
      </div>

      {#if error}
        <p class="error">{error}</p>
      {/if}

      <div class="actions">
        <button class="btn-link" onclick={resetDefaults} disabled={saving}>
          Restore defaults
        </button>
        <div class="actions-right">
          <button class="btn-secondary" onclick={onClose} disabled={saving}>
            Cancel
          </button>
          <button class="btn-primary" onclick={handleSave} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
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
    min-width: 420px;
    max-width: 520px;
    color: var(--fg);
  }

  h2 {
    margin: 0 0 16px 0;
    font-size: 15px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 14px;
  }

  .field.checkbox {
    flex-direction: row;
    align-items: center;
  }

  .field.checkbox label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  label {
    font-size: 12px;
    color: var(--fg-muted);
  }

  input[type="text"] {
    background: var(--bg);
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

  input[type="range"] {
    accent-color: var(--accent);
  }

  small {
    font-size: 11px;
    color: var(--fg-faint);
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }

  .error {
    margin: 0 0 12px 0;
    color: var(--danger);
    font-size: 12px;
  }

  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 6px;
  }

  .actions-right {
    display: flex;
    gap: 8px;
  }

  .btn-secondary,
  .btn-primary,
  .btn-link {
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

  .btn-link {
    background: transparent;
    color: var(--fg-muted);
    padding-left: 0;
    padding-right: 0;
  }

  .btn-link:hover:not(:disabled) {
    color: var(--fg);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
