<script lang="ts">
  import { preferences, savePreferences, type Preferences } from "../preferences";
  import { api, type IntegrationStatus } from "../api";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open, onClose }: Props = $props();

  let fontFamily = $state("");
  let fontSize = $state(13);
  let lineHeight = $state(1.15);
  let cursorBlink = $state(true);
  let bellNotifications = $state(true);
  let restoreSessions = $state(true);
  let editorCommand = $state("");
  let integration = $state<IntegrationStatus | null>(null);
  let integrationBusy = $state(false);
  let integrationMsg = $state<string | null>(null);
  let integrationError = $state<string | null>(null);
  let saving = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) {
      const p = $preferences;
      fontFamily = p.fontFamily;
      fontSize = p.fontSize;
      lineHeight = p.lineHeight;
      cursorBlink = p.cursorBlink;
      bellNotifications = p.bellNotifications;
      restoreSessions = p.restoreSessions;
      editorCommand = p.editorCommand;
      error = null;
      integrationMsg = null;
      integrationError = null;
      void refreshIntegration();
    }
  });

  async function refreshIntegration() {
    try {
      integration = await api.claudeIntegrationStatus();
    } catch (e) {
      integrationError = String(e);
    }
  }

  async function installPlugin() {
    integrationBusy = true;
    integrationMsg = null;
    integrationError = null;
    try {
      await api.installClaudePlugin();
      integrationMsg =
        "Plugin installed. Restart running Claude sessions (or run /reload-plugins in them) to pick it up.";
    } catch (e) {
      integrationError = `Could not install through claude: ${e}. Run this in a terminal instead: ${integration?.manualCommands ?? ""}`;
    } finally {
      integrationBusy = false;
      await refreshIntegration();
    }
  }

  async function handleSave() {
    saving = true;
    error = null;
    const next: Preferences = {
      ...$preferences,
      fontFamily: fontFamily.trim() || $preferences.fontFamily,
      fontSize: clamp(Math.round(fontSize), 8, 32),
      lineHeight: clamp(lineHeight, 1.0, 2.0),
      cursorBlink,
      bellNotifications,
      restoreSessions,
      editorCommand: editorCommand.trim(),
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
    bellNotifications = true;
    restoreSessions = true;
    editorCommand = "";
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

      <div class="field checkbox">
        <label>
          <input type="checkbox" bind:checked={bellNotifications} />
          Notify when a background terminal needs attention
        </label>
      </div>
      <small>
        When a terminal rings the bell or sends a notification, its card lights
        up; if TermPane isn't focused, a system notification fires too — e.g.
        when Claude finishes and waits for you.
      </small>

      <div class="field checkbox">
        <label>
          <input type="checkbox" bind:checked={restoreSessions} />
          Reopen tabs after TermPane quits or crashes
        </label>
      </div>
      <small>
        Brings back tabs, sub-tabs, splits and folders on the next launch, and
        resumes Claude Code sessions that were running (needs the integration
        below).
      </small>

      <div class="field">
        <label for="editor">Open file paths with</label>
        <input
          id="editor"
          type="text"
          bind:value={editorCommand}
          placeholder="code -g {"{file}"}:{"{line}"}:{"{col}"}"
        />
        <small>
          ⌘-click a path like src/app.ts:42 in a terminal to open it. Empty
          uses VS Code (code -g), else the file's default app. Placeholders:
          {"{file}"}, {"{line}"}, {"{col}"}.
        </small>
      </div>

      <section class="integration">
        <h3>Claude Code integration</h3>
        <p class="integration-text">
          One Claude Code plugin: tabs show what Claude is doing and the files
          it changes, sessions can be resumed and restored, Claude can read,
          type into and open TermPane tabs, and /fork opens the copy in a
          sub-tab. Outside TermPane the plugin does nothing.
        </p>
        <div class="integration-row">
          <span class="state" class:on={integration?.pluginInstalled}>
            {integration === null
              ? "Checking…"
              : integration.pluginInstalled
                ? "Plugin installed"
                : "Plugin not installed"}
          </span>
          <button class="btn-secondary" onclick={installPlugin} disabled={integrationBusy}>
            {integrationBusy
              ? "Installing…"
              : integration?.pluginInstalled
                ? "Reinstall plugin"
                : "Install Claude Code plugin"}
          </button>
        </div>
        {#if integration?.legacySetup}
          <small>
            The earlier hooks / MCP server setup is still configured; installing
            the plugin removes it so nothing reports twice.
          </small>
        {/if}
        {#if integrationMsg}
          <p class="integration-ok">{integrationMsg}</p>
        {/if}
        {#if integrationError}
          <p class="error">{integrationError}</p>
        {/if}
      </section>

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
    max-height: calc(100vh - 60px);
    overflow-y: auto;
    color: var(--fg);
  }

  .integration {
    margin: 18px 0 14px 0;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  h3 {
    margin: 0 0 6px 0;
    font-size: 13px;
  }

  .integration-text {
    margin: 0 0 10px 0;
    font-size: 11px;
    color: var(--fg-faint);
  }

  .integration-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 8px;
  }

  .state {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .state.on {
    color: #2ea043;
  }

  .integration-ok {
    margin: 4px 0 0 0;
    font-size: 11px;
    color: var(--fg-muted);
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
