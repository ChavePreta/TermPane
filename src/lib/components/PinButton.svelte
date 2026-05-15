<script lang="ts">
  import { alwaysOnTop } from "../alwaysOnTop";

  function toggle() {
    alwaysOnTop.update((v) => !v);
  }
</script>

<button
  class="pin-btn"
  class:active={$alwaysOnTop}
  onclick={toggle}
  title={$alwaysOnTop ? "Unpin from top" : "Pin on top"}
  aria-label={$alwaysOnTop ? "Unpin from top" : "Pin on top"}
  aria-pressed={$alwaysOnTop}
>
  <svg
    viewBox="0 0 24 24"
    width="14"
    height="14"
    fill={$alwaysOnTop ? "currentColor" : "none"}
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="M12 2v6" />
    <path d="M5 8h14l-2 6H7L5 8z" />
    <path d="M12 14v8" />
  </svg>
</button>

<style>
  .pin-btn {
    -webkit-app-region: no-drag;
    width: 24px;
    height: 24px;
    border-radius: 5px;
    color: var(--fg-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s, color 0.12s;
  }

  .pin-btn:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .pin-btn.active {
    color: var(--accent);
  }

  .pin-btn.active:hover {
    background: var(--bg-hover);
  }
</style>
