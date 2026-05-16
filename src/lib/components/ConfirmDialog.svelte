<script lang="ts">
  type Props = {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    onConfirm: () => void;
    onCancel: () => void;
  };

  let {
    open,
    title,
    message,
    confirmLabel = "Close",
    cancelLabel = "Cancel",
    onConfirm,
    onCancel,
  }: Props = $props();

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") onCancel();
    if (e.key === "Enter") onConfirm();
  }
</script>

<svelte:window onkeydown={handleKey} />

{#if open}
  <div
    class="backdrop"
    onclick={onCancel}
    onkeydown={(e) => e.key === "Escape" && onCancel()}
    role="presentation"
  >
    <div
      class="dialog"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="cd-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 id="cd-title">{title}</h2>
      <p>{message}</p>
      <div class="actions">
        <button class="btn-secondary" onclick={onCancel}>{cancelLabel}</button>
        <button class="btn-danger" onclick={onConfirm}>{confirmLabel}</button>
      </div>
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
    padding: 20px 22px 16px 22px;
    min-width: 320px;
    max-width: 440px;
  }

  h2 {
    margin: 0 0 8px 0;
    font-size: 15px;
    color: var(--fg);
  }

  p {
    margin: 0 0 18px 0;
    color: var(--fg-muted);
    line-height: 1.5;
    white-space: pre-line;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn-secondary,
  .btn-danger {
    padding: 7px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    transition: background 0.12s;
  }

  .btn-secondary {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .btn-secondary:hover {
    background: var(--bg-active);
  }

  .btn-danger {
    background: var(--danger);
    color: #fff;
  }

  .btn-danger:hover {
    filter: brightness(1.1);
  }
</style>
