<script lang="ts">
  import { api, type SessionFiles, type StageResult } from "../api";
  import { terminals } from "../store";

  type Props = {
    terminalId: string | null;
    // Stage right away (the keyboard shortcut) instead of waiting for a click.
    autoStage: boolean;
    onClose: () => void;
  };

  let { terminalId, autoStage, onClose }: Props = $props();

  let sessions = $state<SessionFiles[]>([]);
  let loading = $state(false);
  let busy = $state(false);
  let result = $state<StageResult | null>(null);
  // The file whose diff is shown, and that diff.
  let diffFor = $state<string | null>(null);
  let diffText = $state<string | null>(null);
  let diffError = $state<string | null>(null);

  async function toggleDiff(path: string) {
    if (diffFor === path) {
      diffFor = null;
      return;
    }
    diffFor = path;
    diffText = null;
    diffError = null;
    try {
      const text = await api.fileDiff(path);
      if (diffFor === path) diffText = text;
    } catch (e) {
      if (diffFor === path) diffError = String(e);
    }
  }

  function lineClass(line: string): string {
    if (line.startsWith("+++") || line.startsWith("---")) return "meta";
    if (line.startsWith("@@")) return "hunk";
    if (line.startsWith("+")) return "add";
    if (line.startsWith("-")) return "del";
    return "";
  }
  let error = $state<string | null>(null);

  let label = $derived($terminals.find((t) => t.id === terminalId)?.label ?? "");
  let stageable = $derived(
    sessions.reduce((n, s) => n + s.files.filter((f) => !s.shared.includes(f)).length, 0),
  );
  let total = $derived(sessions.reduce((n, s) => n + s.files.length, 0));

  $effect(() => {
    if (!terminalId) return;
    const id = terminalId;
    result = null;
    error = null;
    void load(id).then(() => {
      if (autoStage && id === terminalId) void stage();
    });
  });

  async function load(id: string) {
    loading = true;
    try {
      sessions = await api.getTerminalFiles(id);
    } catch (e) {
      error = String(e);
      sessions = [];
    } finally {
      loading = false;
    }
  }

  async function stage() {
    if (!terminalId || busy) return;
    busy = true;
    error = null;
    try {
      result = await api.stageTerminalFiles(terminalId);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function clearList() {
    if (!terminalId) return;
    try {
      await api.clearTerminalFiles(terminalId);
      result = null;
      await load(terminalId);
    } catch (e) {
      error = String(e);
    }
  }

  // Long absolute paths: keep the last few segments, full path on hover.
  function shortPath(p: string): string {
    const parts = p.split("/");
    return parts.length > 4 ? "…/" + parts.slice(-3).join("/") : p;
  }

  function handleKey(e: KeyboardEvent) {
    if (terminalId && e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKey} />

{#if terminalId}
  <div class="backdrop" onclick={onClose} role="presentation">
    <div
      class="dialog"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="files-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 id="files-title">Files changed by Claude — {label}</h2>

      {#if loading}
        <p class="muted">Loading…</p>
      {:else if total === 0}
        <p class="muted">
          No files recorded yet. Files appear here when Claude edits or writes
          them in this tab (requires the Claude Code integration in
          Preferences). Changes made through shell commands aren't tracked.
        </p>
      {:else}
        <div class="sessions">
          {#each sessions as s (s.paneId)}
            {#if s.files.length > 0}
              <div class="session">
                <div class="session-head">
                  Session <code>{s.sessionId.slice(0, 8)}</code>
                  <span class="muted">{s.live ? "running" : "ended"}</span>
                </div>
                <ul>
                  {#each s.files as f (f)}
                    <li title={f}>
                      <button class="diff-btn" class:open={diffFor === f} onclick={() => toggleDiff(f)}>
                        {diffFor === f ? "Hide" : "Diff"}
                      </button>
                      <span class="path">{shortPath(f)}</span>
                      {#if s.shared.includes(f)}
                        <span class="badge" title="Another Claude session in this window also wrote this file, so it is never staged automatically.">also edited in another tab</span>
                      {/if}
                      {#if result?.staged.includes(f)}
                        <span class="badge ok">staged</span>
                      {/if}
                    </li>
                    {#if diffFor === f}
                      <li class="diff-row">
                        {#if diffError}
                          <p class="error">{diffError}</p>
                        {:else if diffText === null}
                          <p class="muted">Loading diff…</p>
                        {:else if diffText === ""}
                          <p class="muted">No changes against the last commit.</p>
                        {:else}
                          <pre class="diff">{#each diffText.split("\n") as line}<span class={lineClass(line)}>{line}
</span>{/each}</pre>
                          {#if s.shared.includes(f)}
                            <p class="muted">Another tab also edited this file; this diff includes its changes.</p>
                          {/if}
                        {/if}
                      </li>
                    {/if}
                  {/each}
                </ul>
              </div>
            {/if}
          {/each}
        </div>
        <small>
          Staging adds only the files that no other Claude session touched.
          Edits made through shell commands (sed, formatters, codegen) aren't
          tracked here.
        </small>
      {/if}

      {#if result}
        <p class="result">
          Staged {result.staged.length} file{result.staged.length === 1 ? "" : "s"}{result.shared.length
            ? `; skipped ${result.shared.length} shared with another tab`
            : ""}.
        </p>
        {#each result.errors as err}
          <p class="error">{err}</p>
        {/each}
      {/if}
      {#if error}
        <p class="error">{error}</p>
      {/if}

      <div class="actions">
        <button class="btn-link" onclick={clearList} disabled={total === 0 || busy}>
          Clear list
        </button>
        <div class="actions-right">
          <button class="btn-secondary" onclick={onClose}>Close</button>
          <button class="btn-primary" onclick={stage} disabled={stageable === 0 || busy}>
            {busy ? "Staging…" : `Stage ${stageable} file${stageable === 1 ? "" : "s"}`}
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
    width: min(600px, calc(100vw - 48px));
    max-height: calc(100vh - 80px);
    display: flex;
    flex-direction: column;
    color: var(--fg);
  }

  h2 {
    margin: 0 0 14px 0;
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sessions {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 10px;
  }

  .session-head {
    font-size: 12px;
    margin-bottom: 4px;
    display: flex;
    gap: 8px;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
    font-size: 12px;
    min-width: 0;
  }

  .path {
    font-family: Menlo, Monaco, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .diff-btn {
    flex-shrink: 0;
    font-size: 10.5px;
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--border);
    color: var(--fg-muted);
  }

  .diff-btn:hover,
  .diff-btn.open {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .diff-row {
    display: block;
    padding: 4px 0 8px 0;
  }

  .diff {
    margin: 0;
    max-height: 320px;
    overflow: auto;
    padding: 8px 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: Menlo, Monaco, monospace;
    font-size: 11.5px;
    line-height: 1.4;
    white-space: pre;
    user-select: text;
  }

  .diff .add {
    color: #2ea043;
    background: color-mix(in srgb, #2ea043 10%, transparent);
  }

  .diff .del {
    color: #d1242f;
    background: color-mix(in srgb, #d1242f 10%, transparent);
  }

  .diff .hunk {
    color: var(--accent);
  }

  .diff .meta {
    color: var(--fg-muted);
  }

  .badge {
    flex-shrink: 0;
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, #e5a50a 22%, transparent);
    color: var(--fg);
  }

  .badge.ok {
    background: color-mix(in srgb, #2ea043 25%, transparent);
  }

  code {
    font-family: Menlo, Monaco, monospace;
  }

  .muted {
    color: var(--fg-muted);
    font-size: 12px;
  }

  small {
    font-size: 11px;
    color: var(--fg-faint);
  }

  .result {
    font-size: 12px;
    margin: 10px 0 0 0;
  }

  .error {
    margin: 6px 0 0 0;
    color: var(--danger);
    font-size: 12px;
  }

  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 16px;
  }

  .actions-right {
    display: flex;
    gap: 8px;
  }

  .btn-primary,
  .btn-secondary,
  .btn-link {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
  }

  .btn-primary {
    background: var(--accent);
    color: #fff;
  }

  .btn-secondary {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .btn-link {
    color: var(--fg-muted);
    padding-left: 0;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
