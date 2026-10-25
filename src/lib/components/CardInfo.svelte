<script lang="ts">
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
  import { copyToClipboard } from "../clipboard";

  type Props = {
    terminal: TerminalSnapshot;
    // The card's box; the balloon opens to its right.
    anchor: DOMRect;
    onOpenFiles: () => void;
    onPointerEnter: () => void;
    onPointerLeave: () => void;
  };

  let { terminal, anchor, onOpenFiles, onPointerEnter, onPointerLeave }: Props = $props();

  const STATUS: Record<string, string> = {
    idle: "Claude ready",
    thinking: "Claude is thinking",
    tool: "Claude is working",
    permission: "Claude needs your permission",
    waiting: "Claude is waiting for you",
    done: "Claude finished",
  };

  let claude = $derived(terminalClaude(terminal));
  let status = $derived.by(() => {
    if (claude && claude.status !== "ended") {
      const base = STATUS[claude.status] ?? "";
      return claude.detail ? `${base} — ${claude.detail}` : base;
    }
    const fg = aggregateForeground(terminal, 200);
    return fg ? `Running ${fg}` : "Idle shell";
  });
  let activePane = $derived(
    terminal.panes.find((p) => p.id === terminal.activePane) ?? terminal.panes[0],
  );
  let cwd = $derived(activePane?.cwd ?? null);
  let git = $derived(terminalGit(terminal));
  let ports = $derived(terminalPorts(terminal));
  let files = $derived(terminalFileCount(terminal));
  let memory = $derived(formatBytes(aggregateMemory(terminal)));
  let opened = $derived(
    new Date(terminal.createdAt).toLocaleTimeString(undefined, { hour12: false }),
  );
  let copied = $state(false);

  let top = $derived(Math.max(8, Math.min(anchor.top, window.innerHeight - 320)));
  let left = $derived(anchor.right + 8);

  async function copyCwd() {
    if (!cwd) return;
    if (await copyToClipboard(cwd)) {
      copied = true;
      setTimeout(() => (copied = false), 1200);
    }
  }

  function openPort(url: string) {
    api.openPortUrl(terminal.id, url).catch((err) => console.error("open port:", err));
  }
</script>

<div
  class="balloon"
  role="dialog"
  aria-label={`About ${terminal.label}`}
  tabindex="-1"
  style="top: {top}px; left: {left}px"
  onpointerenter={onPointerEnter}
  onpointerleave={onPointerLeave}
>
  <div class="name">{terminal.label}</div>
  <div class="status" class:needs-you={claude?.status === "permission" || claude?.status === "waiting"}>
    {status}
  </div>

  <dl>
    {#if cwd}
      <dt>Folder</dt>
      <dd class="row">
        <span class="mono path" title={cwd}>{cwd}</span>
        <button class="icon" title={copied ? "Copied" : "Copy the folder path"} aria-label="Copy the folder path" onclick={copyCwd}>
          {#if copied}
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12" /></svg>
          {:else}
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2" /><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1" /></svg>
          {/if}
        </button>
      </dd>
    {/if}

    {#if git}
      <dt>Git</dt>
      <dd class="mono">
        {git.repo} · {git.branch}{#if git.worktree}<span class="muted"> (worktree {git.worktree})</span>{/if}
      </dd>
    {/if}

    {#if ports.length > 0}
      <dt>Servers</dt>
      <dd>
        {#each ports as port (port.address)}
          <div class="row">
            <span>
              {#if port.label}{port.label} · {/if}<strong class="port">port {port.address.split(":").pop()}</strong>
              <span class="muted mono"> {port.url ?? port.address}</span>
            </span>
            {#if port.url}
              <button class="link" title={port.url} onclick={() => openPort(port.url!)}>Open</button>
            {/if}
          </div>
        {/each}
      </dd>
    {/if}

    {#if files > 0}
      <dt>Changed by Claude</dt>
      <dd class="row">
        <span>{files} file{files === 1 ? "" : "s"}</span>
        <button class="link" onclick={onOpenFiles}>Review &amp; stage</button>
      </dd>
    {/if}

    <dt>Tab</dt>
    <dd class="muted">
      opened {opened}{memory ? ` · ${memory}` : ""}{terminal.panes.length > 1 ? ` · ${terminal.panes.length} panes` : ""}
    </dd>
  </dl>
</div>

<style>
  .balloon {
    position: fixed;
    z-index: 950;
    width: 340px;
    max-width: calc(100vw - 16px);
    padding: 12px 14px;
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    color: var(--fg);
    font-size: 12px;
    -webkit-app-region: no-drag;
  }

  .name {
    font-size: 13.5px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .status {
    margin-top: 2px;
    color: var(--fg-muted);
    overflow-wrap: anywhere;
  }

  .status.needs-you {
    color: #c98a00;
  }

  dl {
    margin: 10px 0 0 0;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 12px;
  }

  dt {
    color: var(--fg-faint);
    font-size: 11px;
    padding-top: 1px;
  }

  dd {
    margin: 0;
    min-width: 0;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }

  .row + .row {
    margin-top: 3px;
  }

  .mono {
    font-family: Menlo, Monaco, monospace;
    font-size: 11px;
  }

  .path {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .muted {
    color: var(--fg-muted);
  }

  .port {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .icon {
    flex-shrink: 0;
    width: 22px;
    height: 20px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-muted);
    border: 1px solid var(--border);
  }

  .icon:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .link {
    flex-shrink: 0;
    font-size: 11px;
    color: var(--accent);
    padding: 1px 6px;
    border-radius: 4px;
  }

  .link:hover {
    background: var(--bg-hover);
    text-decoration: underline;
  }
</style>
