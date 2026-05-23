<script lang="ts">
  import TerminalView from "./TerminalView.svelte";
  import Splitter from "./Splitter.svelte";
  import Self from "./PaneTreeView.svelte";
  import { api, type LayoutSnapshot, type TerminalSnapshot } from "../api";

  type Props = {
    node: LayoutSnapshot;
    terminal: TerminalSnapshot;
    terminalActive: boolean;
    path?: number[];
  };

  let { node, terminal, terminalActive, path = [] }: Props = $props();

  let containerEl: HTMLDivElement | null = $state(null);

  // Local ratios override during drag, to avoid a round-trip per frame.
  let localRatios = $state<number[] | null>(null);

  let effectiveRatios = $derived(
    node.type === "split" ? localRatios ?? node.ratios : [],
  );

  async function handleFocus(paneId: string) {
    try {
      await api.focusPane(paneId);
    } catch (e) {
      console.error("focusPane erro:", e);
    }
  }

  function handleDrag(splitterIdx: number, deltaPx: number) {
    if (node.type !== "split" || !containerEl) return;
    const total =
      node.dir === "horizontal"
        ? containerEl.clientWidth
        : containerEl.clientHeight;
    if (total <= 0) return;
    const deltaR = deltaPx / total;
    const base = node.ratios;
    const next = [...base];
    const a = base[splitterIdx];
    const b = base[splitterIdx + 1];
    const newA = clamp(a + deltaR, 0.1, a + b - 0.1);
    const newB = a + b - newA;
    next[splitterIdx] = newA;
    next[splitterIdx + 1] = newB;
    localRatios = next;
  }

  async function commit() {
    if (!localRatios) return;
    const committed = localRatios;
    localRatios = null;
    try {
      await api.setLayoutRatios(terminal.id, path, committed);
    } catch (e) {
      console.error("setLayoutRatios erro:", e);
    }
  }

  function clamp(v: number, lo: number, hi: number) {
    return Math.max(lo, Math.min(hi, v));
  }
</script>

{#if node.type === "leaf"}
  <TerminalView
    paneId={node.id}
    terminalId={terminal.id}
    active={terminal.activePane === node.id}
    {terminalActive}
    onFocus={() => handleFocus(node.id)}
  />
{:else}
  <div
    bind:this={containerEl}
    class="split-container"
    class:horizontal={node.dir === "horizontal"}
    class:vertical={node.dir === "vertical"}
  >
    {#each node.children as child, i (i)}
      <div
        class="split-child"
        style:flex-grow={effectiveRatios[i] ?? 1}
        style:flex-basis="0"
      >
        <Self
          node={child}
          {terminal}
          {terminalActive}
          path={[...path, i]}
        />
      </div>
      {#if i < node.children.length - 1}
        <Splitter
          dir={node.dir}
          onDrag={(d) => handleDrag(i, d)}
          onCommit={commit}
        />
      {/if}
    {/each}
  </div>
{/if}

<style>
  .split-container {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .split-container.horizontal {
    flex-direction: row;
  }

  .split-container.vertical {
    flex-direction: column;
  }

  .split-child {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    display: flex;
  }

  .split-child > :global(*) {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
  }
</style>
