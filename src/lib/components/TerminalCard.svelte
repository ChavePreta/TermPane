<script lang="ts">
  import { tick } from "svelte";
  import type { TerminalSnapshot } from "../api";
  import { api } from "../api";
  import {
    aggregateForeground,
    aggregateMemory,
    formatBytes,
    terminalClaude,
    terminalFileCount,
    terminalGit,
    terminalPorts,
  } from "../store";
  import { attentionTerminals } from "../attention";
  import { paneProgress, terminalProgress } from "../progress";
  import CardInfo from "./CardInfo.svelte";

  type Props = {
    terminal: TerminalSnapshot;
    active: boolean;
    dragging: boolean;
    depth: number;
    hasChildren: boolean;
    collapsed: boolean;
    onToggleCollapse: () => void;
    onSelect: () => void;
    onRequestClose: () => void;
    onContextMenu: (e: MouseEvent) => void;
    onOpenFiles: () => void;
  };

  let {
    terminal,
    active,
    dragging,
    depth,
    hasChildren,
    collapsed,
    onToggleCollapse,
    onSelect,
    onRequestClose,
    onContextMenu,
    onOpenFiles,
  }: Props = $props();

  const STATUS_TEXT: Record<string, string> = {
    idle: "Claude ready",
    thinking: "thinking…",
    tool: "working…",
    permission: "needs permission",
    waiting: "waiting for you",
    done: "done",
  };

  let claude = $derived(terminalClaude(terminal));
  let claudeLive = $derived(claude !== null && claude.status !== "ended");
  let claudeBusy = $derived(claude?.status === "thinking" || claude?.status === "tool");
  let claudeText = $derived(
    claude && claudeLive
      ? claude.status === "tool" || claude.status === "permission"
        ? (claude.detail ?? STATUS_TEXT[claude.status])
        : STATUS_TEXT[claude.status]
      : null,
  );
  let fileCount = $derived(terminalFileCount(terminal));
  let git = $derived(terminalGit(terminal));
  let ports = $derived(terminalPorts(terminal));
  let gitText = $derived(
    git ? (git.worktree ? `worktree: ${git.worktree}` : git.branch) : null,
  );

  // --- The info balloon: after 2 s of hover, or at once from the ⓘ button.
  // It stays while the pointer is over the card or the balloon itself.
  let cardEl: HTMLDivElement | null = $state(null);
  let infoAnchor = $state<DOMRect | null>(null);
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | null = null;

  function showInfo() {
    if (!cardEl || editing || dragging) return;
    infoAnchor = cardEl.getBoundingClientRect();
  }

  function hideInfo() {
    if (hoverTimer) clearTimeout(hoverTimer);
    if (hideTimer) clearTimeout(hideTimer);
    hoverTimer = hideTimer = null;
    infoAnchor = null;
  }

  function cancelHide() {
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = null;
  }

  function scheduleHide() {
    cancelHide();
    hideTimer = setTimeout(hideInfo, 250);
  }

  function handleEnter() {
    hovering = true;
    cancelHide();
    if (!infoAnchor && !hoverTimer) hoverTimer = setTimeout(showInfo, 2000);
  }

  function handleLeave() {
    hovering = false;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = null;
    if (infoAnchor) scheduleHide();
  }

  function toggleInfo(e: MouseEvent) {
    e.stopPropagation();
    if (infoAnchor) hideInfo();
    else showInfo();
  }

  $effect(() => {
    if (dragging || editing) hideInfo();
  });

  let hovering = $state(false);
  let editing = $state(false);
  let inputEl: HTMLInputElement | null = $state(null);
  let editValue = $state("");

  let foreground = $derived(aggregateForeground(terminal));
  let progress = $derived(terminalProgress($paneProgress, terminal.id));
  let progressText = $derived(
    progress && progress.state !== "indeterminate" ? `${progress.percent}%` : null,
  );
  let subtitle = $derived(
    claudeText ?? ([foreground, progressText].filter(Boolean).join(" · ") || null),
  );
  let memoryLabel = $derived(formatBytes(aggregateMemory(terminal)));
  let attentionEntry = $derived($attentionTerminals.get(terminal.id));
  let pulseKey = $derived(attentionEntry?.nonce);
  let showDot = $derived(attentionEntry !== undefined);
  let persistent = $derived(!!attentionEntry?.persistent && !active);

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
    hideInfo();
    onSelect();
  }

  function handleToggle(e: MouseEvent) {
    e.stopPropagation();
    onToggleCollapse();
  }

  let claudePane = $derived(
    claudeLive ? terminal.panes.find((p) => p.claude === claude) : undefined,
  );
  let remote = $derived(claude?.remote ?? false);

  // Off: Claude asks in the pane whether to disconnect, so show the pane.
  async function handleRemote(e: MouseEvent) {
    if (!remote) e.stopPropagation();
    if (!claudePane) return;
    try {
      await api.toggleClaudeRemote(claudePane.id);
    } catch (err) {
      console.error("toggleClaudeRemote:", err);
    }
  }

  function handleFiles(e: MouseEvent) {
    e.stopPropagation();
    onOpenFiles();
  }
</script>

<div
  bind:this={cardEl}
  class="card"
  class:active
  class:dragging
  class:editing
  class:attention={persistent}
  class:unread={terminal.markedUnread}
  data-card-id={terminal.id}
  style="padding-left: {10 + depth * 16}px"
  role="button"
  tabindex="0"
  onmouseenter={handleEnter}
  onmouseleave={handleLeave}
  onclick={handleClick}
  ondblclick={startEdit}
  onkeydown={handleKey}
  oncontextmenu={(e) => {
    e.preventDefault();
    onContextMenu(e);
  }}
>
  {#if hasChildren}
    <button
      class="card-toggle"
      class:collapsed
      title={collapsed ? "Show sub-tabs" : "Hide sub-tabs"}
      aria-label={collapsed ? "Show sub-tabs" : "Hide sub-tabs"}
      onclick={handleToggle}
    >
      <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>
  {/if}
  {#if claudeBusy}
    <span class="claude-spinner" title="Claude is working" aria-hidden="true"></span>
  {:else if claudeLive && claude}
    <span
      class="claude-state {claude.status}"
      title={claudeText ?? ""}
      aria-hidden="true"
    ></span>
  {/if}
  {#if terminal.markedUnread}
    <span class="unread-mark" title="Marked as unread" aria-label="Marked as unread"></span>
  {/if}
  {#if showDot}
    {#key pulseKey}
      <span
        class="attention-dot"
        class:persistent
        title="Activity — waiting for you"
        aria-hidden="true"
      ></span>
    {/key}
  {/if}
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
    <span class="card-body">
      <span class="card-text">
        <span class="card-name">{terminal.label}</span>
        {#if subtitle}
          <span
            class="card-cmd"
            class:claude-text={claudeText !== null}
            class:needs-you={claude?.status === "permission" || claude?.status === "waiting"}
          > — {subtitle}</span>
        {/if}
      </span>
      {#if gitText}
        <span class="card-sub"><span class="card-git">{gitText}</span></span>
      {/if}
    </span>
    {#if fileCount > 0}
      <button
        class="card-files"
        title="Files changed by Claude in this tab"
        aria-label={`${fileCount} files changed by Claude`}
        onclick={handleFiles}
      >
        <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
        </svg>
        {fileCount}
      </button>
    {/if}
    {#if memoryLabel}
      <span class="card-mem" title="Memory (shell + descendants)">{memoryLabel}</span>
    {/if}
    {#if claudePane}
      <button
        class="card-remote"
        class:on={remote}
        title={remote
          ? "Remote Control is on — click to disconnect (/remote-control)"
          : "Remote Control is off — click to connect (/remote-control)"}
        aria-label={remote ? "Disconnect Remote Control" : "Connect Remote Control"}
        aria-pressed={remote}
        onclick={handleRemote}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="6" y="2" width="12" height="20" rx="2" />
          <line x1="11" y1="18" x2="13" y2="18" />
          {#if !remote}
            <line x1="3" y1="3" x2="21" y2="21" />
          {/if}
        </svg>
      </button>
    {/if}
    <button
        class="card-info"
        class:open={infoAnchor !== null}
        title="About this tab"
        aria-label="About this tab"
        onclick={toggleInfo}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="16" x2="12" y2="12" />
          <line x1="12" y1="8" x2="12.01" y2="8" />
        </svg>
      </button>
    <!-- Always takes its place; only shows on hover, so nothing moves. -->
    {#if progress}
      <span
        class="card-progress {progress.state}"
        role="progressbar"
        aria-label="Progress"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={progress.state === "indeterminate" ? undefined : progress.percent}
      >
        <span
          class="card-progress-fill"
          style:width={progress.state === "indeterminate" ? null : `${progress.percent}%`}
        ></span>
      </span>
    {/if}
    <button
      class="card-close"
      class:shown={hovering}
      title="Close terminal"
      aria-label="Close terminal"
      tabindex={hovering ? 0 : -1}
      onclick={handleClose}
    >
      ×
    </button>
  {/if}
</div>
{#if infoAnchor}
  <CardInfo
    {terminal}
    anchor={infoAnchor}
    onOpenFiles={() => {
      hideInfo();
      onOpenFiles();
    }}
    onPointerEnter={cancelHide}
    onPointerLeave={scheduleHide}
  />
{/if}

<style>
  .card {
    -webkit-app-region: no-drag;
    position: relative;
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

  .card-progress {
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: 2px;
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    overflow: hidden;
    pointer-events: none;
  }

  .card-progress-fill {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.2s ease-out;
  }

  .card-progress.error .card-progress-fill {
    background: var(--danger);
  }

  .card-progress.paused .card-progress-fill {
    background: #e5a50a;
  }

  .card-progress.indeterminate .card-progress-fill {
    width: 30%;
    animation: progress-slide 1.2s ease-in-out infinite;
  }

  @keyframes progress-slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
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

  .card.attention {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }

  .attention-dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    /* Hidden by default; the pulse animation makes it visible for ~3s. A
       persistent dot (unattended terminal) overrides this to stay visible. */
    opacity: 0;
    /* Pulse three times (~3s) on each new bell. */
    animation: attention-pulse 1s ease-in-out 3;
  }

  .attention-dot.persistent {
    opacity: 1;
  }

  @keyframes attention-pulse {
    0% {
      opacity: 1;
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 65%, transparent);
    }
    70% {
      opacity: 1;
      box-shadow: 0 0 0 6px transparent;
    }
    100% {
      opacity: 1;
      box-shadow: 0 0 0 0 transparent;
    }
  }

  .card.unread .card-name {
    font-weight: 700;
  }

  .card-toggle {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    margin-left: -5px;
    margin-right: -3px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--fg);
    border-radius: 4px;
    transition: transform 0.12s, background 0.12s;
  }

  .card-toggle:hover {
    background: var(--bg-hover);
  }

  .card-toggle.collapsed {
    transform: rotate(-90deg);
  }

  .card-toggle:hover {
    color: var(--fg);
  }

  .claude-spinner {
    flex-shrink: 0;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 2px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-top-color: var(--accent);
    animation: claude-spin 0.8s linear infinite;
  }

  @keyframes claude-spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .claude-spinner {
      animation-duration: 2.4s;
    }
  }

  .claude-state {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--fg-faint);
  }

  .claude-state.permission {
    background: #e5a50a;
    box-shadow: 0 0 0 2px color-mix(in srgb, #e5a50a 35%, transparent);
  }

  .claude-state.waiting {
    background: var(--accent);
  }

  .claude-state.done {
    background: #2ea043;
  }

  .unread-mark {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    box-sizing: border-box;
  }

  .card-cmd.claude-text {
    font-style: normal;
  }

  .card-cmd.needs-you {
    color: #c98a00;
    font-weight: 500;
  }

  .card-files {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 11px;
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
    border: 1px solid var(--border);
    transition: background 0.12s, color 0.12s;
  }

  .card-remote {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: var(--fg-muted);
    opacity: 0.55;
    transition: background 0.12s, color 0.12s, opacity 0.12s;
  }

  .card-remote.on {
    color: var(--accent);
    opacity: 1;
  }

  .card-remote:hover {
    background: var(--bg-hover);
    opacity: 1;
  }

  .card-files:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .card-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .card-sub {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    font-size: 11px;
    color: var(--fg-muted);
  }

  .card-git {
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: Menlo, Monaco, monospace;
    font-size: 10.5px;
  }

  .card-text {
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

  .card-info {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    color: var(--fg-muted);
    transition: background 0.12s, color 0.12s, opacity 0.12s;
  }

  .card-info {
    width: 18px;
    height: 18px;
    opacity: 0.45;
  }

  .card:hover .card-info,
  .card-info.open {
    opacity: 1;
  }

  .card-info:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .card-close {
    visibility: hidden;
  }

  .card-close.shown {
    visibility: visible;
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
