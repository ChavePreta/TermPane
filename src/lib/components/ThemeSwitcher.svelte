<script lang="ts">
  import { themeChoice, type ThemeChoice } from "../theme";

  const order: ThemeChoice[] = ["system", "light", "dark"];
  const labels: Record<ThemeChoice, string> = {
    system: "System",
    light: "Light",
    dark: "Dark",
  };

  function cycle() {
    themeChoice.update((c) => order[(order.indexOf(c) + 1) % order.length]);
  }
</script>

<button
  class="theme-toggle"
  onclick={cycle}
  title="Theme: {labels[$themeChoice]} (click to cycle)"
  aria-label="Toggle theme (current: {labels[$themeChoice]})"
>
  {#if $themeChoice === "light"}
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
    </svg>
  {:else if $themeChoice === "dark"}
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
  {:else}
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <rect x="2" y="4" width="20" height="14" rx="2" />
      <path d="M8 21h8M12 18v3" />
    </svg>
  {/if}
</button>

<style>
  .theme-toggle {
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

  .theme-toggle:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }
</style>
