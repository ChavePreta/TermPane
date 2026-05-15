<script lang="ts">
  import type { SplitDir } from "../api";

  type Props = {
    dir: SplitDir;
    onDrag: (deltaPx: number) => void;
    onCommit: () => void;
  };

  let { dir, onDrag, onCommit }: Props = $props();

  let dragging = $state(false);
  let startPos = 0;
  let lastDelta = 0;

  function start(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    dragging = true;
    startPos = dir === "horizontal" ? e.clientX : e.clientY;
    lastDelta = 0;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
  }

  function move(e: PointerEvent) {
    if (!dragging) return;
    const cur = dir === "horizontal" ? e.clientX : e.clientY;
    const delta = cur - startPos;
    if (delta !== lastDelta) {
      lastDelta = delta;
      onDrag(delta);
    }
  }

  function end() {
    if (!dragging) return;
    dragging = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    onCommit();
  }
</script>

<div
  class="splitter"
  class:horizontal={dir === "horizontal"}
  class:vertical={dir === "vertical"}
  class:dragging
  onpointerdown={start}
  role="separator"
  aria-orientation={dir === "horizontal" ? "vertical" : "horizontal"}
></div>

<style>
  .splitter {
    background: transparent;
    flex-shrink: 0;
    transition: background 0.12s;
    position: relative;
  }

  .splitter:hover,
  .splitter.dragging {
    background: var(--accent);
  }

  .splitter.horizontal {
    width: 4px;
    cursor: ew-resize;
  }

  .splitter.vertical {
    height: 4px;
    cursor: ns-resize;
  }
</style>
