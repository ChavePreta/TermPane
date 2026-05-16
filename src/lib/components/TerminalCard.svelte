<script lang="ts">
  import { tick } from "svelte";
  import type { TerminalSnapshot } from "../api";
  import { api } from "../api";
  import { aggregateForeground, aggregateMemory, formatBytes } from "../store";

  type Props = {
    terminal: TerminalSnapshot;
    active: boolean;
    dragging: boolean;
    onSelect: () => void;
    onRequestClose: () => void;
  };

  let { terminal, active, dragging, onSelect, onRequestClose }: Props = $props();

  let hovering = $state(false);
  let editing = $state(false);
  let inputEl: HTMLInputElement | null = $state(null);
  let editValue = $state("");

  let foreground = $derived(aggregateForeground(terminal));
  let memoryLabel = $derived(formatBytes(aggregateMemory(terminal)));

  function formatTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleTimeString(undefined, { hour12: false });
  }

  async function startEdit() {
    if (editing) return;
    editValue = terminal.label;
    editing = true;
    await tick();
    inputEl?.focus();
    inputEl?.select();
  }

  async function commitEdit() {
    if (!editing) return;
    const v = editValue.trim();
    editing = false;
    if (!v || v === terminal.label) return;
    try {
      await api.renameTerminal(terminal.id, v);
    } catch (e) {
      console.error("renameTerminal:", e);
    }
  }

  function cancelEdit() {
    editing = false;
  }

  function handleInputKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      commitEdit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (editing) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect();
    }
    if (e.key === "F2") {
      e.preventDefault();
      startEdit();
    }
  }

  function handleClose(e: MouseEvent) {
    e.stopPropagation();
    onRequestClose();
  }

  function handleClick() {
    if (editing) return;
    onSelect();
  }
</script>

<div
  class="card"
  class:active
  class:dragging
  class:editing
  data-card-id={terminal.id}
  role="button"
  tabindex="0"
  onmouseenter={() => (hovering = true)}
  onmouseleave={() => (hovering = false)}
  onclick={handleClick}
  ondblclick={startEdit}
  onkeydown={handleKey}
>
  {#if editing}
    <input
      class="card-edit"
      bind:this={inputEl}
      bind:value={editValue}
      onkeydown={handleInputKey}
      onblur={commitEdit}
      onclick={(e) => e.stopPropagation()}
    />
  {:else}
    <span class="card-text" title={terminal.label + (foreground ? " — " + foreground : "")}>
      <span class="card-name">{terminal.label}</span>
      {#if foreground}
        <span class="card-cmd"> — {foreground}</span>
      {/if}
    </span>
    {#if memoryLabel && !hovering}
      <span class="card-mem" title="Memory (shell + descendants)">{memoryLabel}</span>
    {/if}
  {/if}
  {#if hovering && !editing}
    <span class="card-meta">
      <span class="card-time">{formatTime(terminal.createdAt)}</span>
      <button
        class="card-close"
        title="Close terminal"
        aria-label="Close terminal"
        onclick={handleClose}
      >
        ×
      </button>
    </span>
  {/if}
</div>

<style>
  .card {
    -webkit-app-region: no-drag;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 6px;
    color: var(--fg);
    text-align: left;
    width: 100%;
    transition: background 0.12s, opacity 0.12s;
    overflow: hidden;
    user-select: none;
    cursor: pointer;
  }

  .card:hover {
    background: var(--bg-hover);
  }

  .card:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .card.active {
    background: var(--bg-active);
  }

  .card.dragging {
    opacity: 0.45;
  }

  .card.editing {
    cursor: text;
  }

  .card-text {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }

  .card-name {
    font-weight: 500;
  }

  .card-cmd {
    font-style: italic;
    color: var(--fg-muted);
    font-weight: 400;
  }

  .card-edit {
    flex: 1;
    min-width: 0;
    background: var(--bg);
    border: 1px solid var(--accent);
    border-radius: 4px;
    padding: 4px 6px;
    color: var(--fg);
    font-family: inherit;
    font-size: 13px;
    outline: none;
  }

  .card-mem {
    flex-shrink: 0;
    font-size: 11px;
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
    padding-left: 6px;
  }

  .card-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .card-time {
    font-size: 11px;
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }

  .card-close {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    line-height: 1;
    color: var(--fg-muted);
    transition: background 0.12s, color 0.12s;
  }

  .card-close:hover {
    background: var(--danger);
    color: #fff;
  }
</style>
