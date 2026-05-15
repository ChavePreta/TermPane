<script lang="ts">
  import { openExternal } from "../api";

  const WEBSITE_URL = "https://termpane.com";
  const DONATE_URL = "https://buymeacoffee.com/chavepreta";
  const GITHUB_URL = "https://github.com/ChavePreta/TermPane";

  function open(url: string) {
    openExternal(url).catch((e) => console.error("openExternal:", e));
  }
</script>

<div class="link-buttons">
  <button
    class="link-btn website"
    onclick={() => open(WEBSITE_URL)}
    title="Visit termpane.com"
    aria-label="Visit termpane.com"
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="10" />
      <line x1="2" y1="12" x2="22" y2="12" />
      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
    </svg>
  </button>
  <button
    class="link-btn donate"
    onclick={() => open(DONATE_URL)}
    title="Support development"
    aria-label="Support development on Buy Me a Coffee"
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M4 10h13a4 4 0 0 1 0 8h-1" />
      <path d="M4 10v8a3 3 0 0 0 3 3h6a3 3 0 0 0 3-3v-8" />
      <path d="M7 6c0-1.5 1-2 1-3" />
      <path d="M11 6c0-1.5 1-2 1-3" />
    </svg>
  </button>
  <button
    class="link-btn github"
    onclick={() => open(GITHUB_URL)}
    title="View on GitHub"
    aria-label="View on GitHub"
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" aria-hidden="true">
      <path d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.38 7.86 10.9.58.1.79-.25.79-.56v-2.16c-3.2.7-3.87-1.36-3.87-1.36-.52-1.33-1.28-1.69-1.28-1.69-1.05-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.27-5.24-5.66 0-1.25.45-2.27 1.18-3.07-.12-.29-.51-1.46.11-3.04 0 0 .96-.31 3.15 1.17a10.9 10.9 0 0 1 5.74 0c2.19-1.48 3.15-1.17 3.15-1.17.62 1.58.23 2.75.11 3.04.74.8 1.18 1.82 1.18 3.07 0 4.4-2.69 5.36-5.25 5.65.41.35.78 1.04.78 2.1v3.11c0 .31.21.67.8.55C20.21 21.38 23.5 17.08 23.5 12 23.5 5.65 18.35.5 12 .5z"/>
    </svg>
  </button>
</div>

<style>
  .link-buttons {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding-top: 6px;
    margin-top: 6px;
    border-top: 1px solid var(--border);
  }

  .link-btn {
    -webkit-app-region: no-drag;
    width: 26px;
    height: 26px;
    border-radius: 5px;
    color: var(--fg-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s, color 0.12s;
  }

  .link-btn:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .link-btn.donate:hover {
    color: #ffdd00;
  }
</style>
