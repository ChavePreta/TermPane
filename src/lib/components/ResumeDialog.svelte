<script lang="ts">
  import { tick } from "svelte";
  import { api, type ClaudeSession } from "../api";
  import { selectTerminal } from "../store";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open, onClose }: Props = $props();

  let sessions = $state<ClaudeSession[]>([]);
  let projectCwd = $state<string | null>(null);
  let onlyProject = $state(true);
  let query = $state("");
  let loading = $state(false);
  let error = $state<string | null>(null);
  let searchEl: HTMLInputElement | null = $state(null);

  // Same project when either directory contains the other — covers worktrees
  // under <repo>/.claude/worktrees and sessions started in a subfolder.
  function sameProject(a: string, b: string): boolean {
    const norm = (p: string) => p.replace(/\/+$/, "");
    const x = norm(a);
    const y = norm(b);
    return x === y || x.startsWith(y + "/") || y.startsWith(x + "/");
  }

  let inProject = $derived(
    projectCwd ? sessions.filter((s) => sameProject(s.cwd, projectCwd!)) : [],
  );
  let shown = $derived.by(() => {
    const base = onlyProject && inProject.length > 0 ? inProject : sessions;
    const q = query.trim().toLowerCase();
    if (!q) return base;
    return base.filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        s.cwd.toLowerCase().includes(q) ||
        (s.gitBranch ?? "").toLowerCase().includes(q),
    );
  });

  $effect(() => {
    if (!open) return;
    query = "";
    error = null;
    void load();
  });

  async function load() {
    loading = true;
    try {
      const [list, cwd] = await Promise.all([api.listClaudeSessions(), api.activeCwd()]);
      sessions = list;
      projectCwd = cwd;
      onlyProject = true;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
      await tick();
      searchEl?.focus();
    }
  }

  async function resume(s: ClaudeSession) {
    onClose();
    try {
      const snap = await api.openTerminal({
        cwd: s.cwd,
        command: `claude --resume ${s.id}`,
        name: s.title.length > 32 ? s.title.slice(0, 31) + "…" : s.title,
        autoLabel: true,
      });
      await selectTerminal(snap.id);
    } catch (e) {
      console.error("resume session:", e);
    }
  }

  function ago(iso: string): string {
    const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins} min ago`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return `${hours} h ago`;
    const days = Math.round(hours / 24);
    return days === 1 ? "yesterday" : `${days} days ago`;
  }

  function shortDir(p: string): string {
    const m = p.match(/^\/(Users|home)\/[^/]+(\/.*)?$/);
    const rel = m ? "~" + (m[2] ?? "") : p;
    const parts = rel.split("/");
    return parts.length > 4 ? "…/" + parts.slice(-3).join("/") : rel;
  }

  function handleKey(e: KeyboardEvent) {
    if (open && e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  function handleSearchKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter" && shown[0]) {
      e.preventDefault();
      void resume(shown[0]);
    } else if (e.key === "Escape") {
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
      aria-labelledby="resume-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 id="resume-title">Resume a Claude session</h2>

      <div class="toolbar">
        <input
          bind:this={searchEl}
          bind:value={query}
          type="search"
          placeholder="Search by title, folder or branch"
          aria-label="Search sessions"
          onkeydown={handleSearchKey}
        />
        {#if projectCwd && inProject.length > 0}
          <div class="seg" role="group" aria-label="Scope">
            <button aria-pressed={onlyProject} onclick={() => (onlyProject = true)}>
              This project
            </button>
            <button aria-pressed={!onlyProject} onclick={() => (onlyProject = false)}>
              All
            </button>
          </div>
        {/if}
      </div>

      {#if loading}
        <p class="muted">Reading Claude Code sessions…</p>
      {:else if error}
        <p class="error">{error}</p>
      {:else if shown.length === 0}
        <p class="muted">
          {sessions.length === 0
            ? "No Claude Code sessions found in ~/.claude/projects."
            : "No session matches the search."}
        </p>
      {:else}
        <ul class="list">
          {#each shown as s (s.id)}
            <li>
              <button class="row" onclick={() => resume(s)} title={`${s.cwd}\nclaude --resume ${s.id}`}>
                <span class="title">{s.title}</span>
                <span class="meta">
                  <span>{shortDir(s.cwd)}</span>
                  {#if s.gitBranch}<span class="branch">{s.gitBranch}</span>{/if}
                  <span class="when">{ago(s.modifiedAt)}</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <small>Opens a new tab in the session's folder and runs <code>claude --resume</code>.</small>
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
    width: min(640px, calc(100vw - 48px));
    max-height: calc(100vh - 80px);
    display: flex;
    flex-direction: column;
    color: var(--fg);
  }

  h2 {
    margin: 0 0 12px 0;
    font-size: 15px;
  }

  .toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 10px;
  }

  input[type="search"] {
    flex: 1;
    min-width: 0;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 7px 9px;
    color: var(--fg);
    font-family: inherit;
    font-size: 13px;
    outline: none;
  }

  input[type="search"]:focus {
    border-color: var(--accent);
  }

  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 5px;
    overflow: hidden;
  }

  .seg button {
    padding: 0 10px;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .seg button[aria-pressed="true"] {
    background: var(--accent);
    color: #fff;
  }

  .list {
    list-style: none;
    margin: 0 0 10px 0;
    padding: 0;
    overflow-y: auto;
  }

  .row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    padding: 7px 9px;
    border-radius: 6px;
    text-align: left;
    color: var(--fg);
    min-width: 0;
  }

  .row:hover,
  .row:focus-visible {
    background: var(--bg-hover);
    outline: none;
  }

  .title {
    font-size: 13px;
    font-weight: 500;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    gap: 10px;
    font-size: 11px;
    color: var(--fg-muted);
    max-width: 100%;
    overflow: hidden;
    white-space: nowrap;
  }

  .branch {
    font-family: Menlo, Monaco, monospace;
  }

  .when {
    font-variant-numeric: tabular-nums;
  }

  .muted {
    color: var(--fg-muted);
    font-size: 12px;
  }

  .error {
    color: var(--danger);
    font-size: 12px;
  }

  small {
    font-size: 11px;
    color: var(--fg-faint);
  }

  code {
    font-family: Menlo, Monaco, monospace;
  }
</style>
