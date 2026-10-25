<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { api, type BackgroundTask } from "../api";

  type Props = {
    paneId: string;
    tasks: BackgroundTask[];
    onClose: () => void;
  };

  let { paneId, tasks, onClose }: Props = $props();

  // Newest first: the task just started is usually the one to watch.
  let ordered = $derived([...tasks].reverse());
  let selected = $state<string | null>(null);
  let text = $state("");
  let active = $state(false);
  let missing = $state(false);
  let error = $state<string | null>(null);
  let outputEl: HTMLPreElement | null = $state(null);
  // Follow new output only while the reader is at the bottom.
  let following = true;
  let timer: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    if (selected === null && ordered[0]) selected = ordered[0].id;
  });

  $effect(() => {
    const id = selected;
    if (!id) return;
    text = "";
    following = true;
    void refresh(id);
    timer = setInterval(() => void refresh(id), 1000);
    return () => {
      if (timer) clearInterval(timer);
      timer = null;
    };
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  async function refresh(id: string) {
    try {
      const out = await api.readBackgroundOutput(paneId, id);
      if (id !== selected) return;
      error = null;
      missing = out.path === null;
      active = out.active;
      if (out.text !== text) {
        text = out.text;
        if (following) {
          await tick();
          if (outputEl) outputEl.scrollTop = outputEl.scrollHeight;
        }
      }
    } catch (e) {
      error = String(e);
    }
  }

  function handleScroll() {
    if (!outputEl) return;
    following = outputEl.scrollHeight - outputEl.scrollTop - outputEl.clientHeight < 24;
  }

  function startedAt(iso: string) {
    return new Date(iso).toLocaleTimeString(undefined, { hour12: false });
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKey} />

<div class="backdrop" onclick={onClose} role="presentation">
  <div
    class="dialog"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="bg-title"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <header>
      <h2 id="bg-title">Running in the background</h2>
      <span class="readonly">View only</span>
    </header>
    <div class="body">
      <ul class="tasks">
        {#each ordered as t (t.id)}
          <li>
            <button class:selected={t.id === selected} onclick={() => (selected = t.id)}>
              <span class="kind">{t.kind === "agent" ? "Agent" : "Command"}</span>
              <span class="desc">{t.description || t.id}</span>
              <span class="when">started {startedAt(t.startedAt)}</span>
            </button>
          </li>
        {/each}
      </ul>
      <div class="viewer">
        <div class="viewer-head">
          {#if active}
            <span class="live"><span class="pulse"></span>writing output</span>
          {:else if !missing}
            <span class="idle">no new output in the last 10 s</span>
          {/if}
        </div>
        {#if error}
          <p class="note error">{error}</p>
        {:else if missing}
          <p class="note">Claude hasn't written any output for this task yet, or its output file was removed.</p>
        {:else}
          <pre bind:this={outputEl} onscroll={handleScroll}>{text || "(no output yet)"}</pre>
        {/if}
      </div>
    </div>
    <div class="actions">
      <button class="btn-secondary" onclick={onClose}>Close</button>
    </div>
  </div>
</div>

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
    padding: 18px 20px 14px 20px;
    width: min(980px, calc(100vw - 48px));
    height: min(640px, calc(100vh - 60px));
    display: flex;
    flex-direction: column;
    color: var(--fg);
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 12px;
  }

  h2 {
    margin: 0;
    font-size: 15px;
  }

  .readonly {
    font-size: 11px;
    color: var(--fg-muted);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 6px;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 240px 1fr;
    gap: 12px;
  }

  .tasks {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    padding-right: 8px;
  }

  .tasks button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    padding: 7px 8px;
    border-radius: 6px;
    text-align: left;
    color: var(--fg);
  }

  .tasks button:hover {
    background: var(--bg-hover);
  }

  .tasks button.selected {
    background: var(--bg-active);
  }

  .kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }

  .desc {
    font-size: 12.5px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .when {
    font-size: 11px;
    color: var(--fg-faint);
    font-variant-numeric: tabular-nums;
  }

  .viewer {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .viewer-head {
    height: 18px;
    font-size: 11px;
    color: var(--fg-muted);
  }

  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: #2ea043;
  }

  .pulse {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #2ea043;
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }

  pre {
    flex: 1;
    min-height: 0;
    margin: 0;
    overflow: auto;
    padding: 10px 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: Menlo, Monaco, monospace;
    font-size: 12px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }

  .note {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .error {
    color: var(--danger);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 12px;
  }

  .btn-secondary {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    background: var(--bg-hover);
    color: var(--fg);
  }
</style>
