/**
 * Puts text on the clipboard. The async Clipboard API first; WKWebView can
 * refuse it outside a direct user gesture, so fall back to a hidden textarea
 * and execCommand — appended to <body>, outside any pane, so a pane's own
 * copy handler doesn't rewrite it.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    return ok;
  }
}
