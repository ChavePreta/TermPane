/**
 * Every keyboard shortcut, grouped for the Keyboard Shortcuts dialog. The one
 * list to update when a shortcut is added or changed (handlers live in
 * `routes/+page.svelte` and `src-tauri/src/menu.rs`).
 */
export type ShortcutEntry = { mac: string; linux: string; label: string };
export type ShortcutGroup = { title: string; items: ShortcutEntry[] };

export const shortcutGroups: ShortcutGroup[] = [
  {
    title: "Tabs",
    items: [
      { mac: "⌘T", linux: "Ctrl+Shift+T", label: "New tab" },
      { mac: "⌘⇧T", linux: "Ctrl+Shift+Alt+T", label: "New sub-tab of the current tab" },
      { mac: "⌘⇧← / ⌘⇧→", linux: "Ctrl+Shift+Alt+← / →", label: "Previous / next tab" },
      { mac: "⌘J", linux: "Ctrl+Shift+J", label: "Jump to the next tab that needs you" },
      { mac: "⌘⇧U", linux: "Ctrl+Shift+Alt+U", label: "Mark tab as unread / read" },
      { mac: "F2", linux: "F2", label: "Rename tab (or double-click)" },
      { mac: "⌘W", linux: "Ctrl+Shift+W", label: "Close pane / tab" },
    ],
  },
  {
    title: "Claude Code",
    items: [
      { mac: "⌘↑ / ⌘↓", linux: "Ctrl+Shift+↑ / ↓", label: "Previous / next prompt you typed" },
      { mac: "⌘⇧L", linux: "Ctrl+Shift+Alt+L", label: "List your prompts in this pane" },
      { mac: "⌘⇧S", linux: "Ctrl+Shift+Alt+S", label: "Stage the files Claude changed in this tab" },
      { mac: "⌘⇧O", linux: "Ctrl+Shift+Alt+O", label: "Resume a Claude session" },
      { mac: "/fork", linux: "/fork", label: "Fork the session into a sub-tab (typed in Claude)" },
    ],
  },
  {
    title: "Splits",
    items: [
      { mac: "⌘D", linux: "Ctrl+Shift+D", label: "Split side by side" },
      { mac: "⌘⇧D", linux: "Ctrl+Shift+Alt+D", label: "Split stacked" },
      { mac: "⌘/", linux: "Ctrl+Shift+/", label: "Flip the split's orientation" },
      { mac: "⌘⇧E", linux: "Ctrl+Shift+Alt+E", label: "Move pane to its own tab" },
    ],
  },
  {
    title: "Scrolling",
    items: [
      { mac: "⌘F", linux: "Ctrl+Shift+F", label: "Find in the scrollback" },
      { mac: "⌘G / ⌘⇧G", linux: "Ctrl+Shift+G / +Alt+G", label: "Next / previous match" },
      { mac: "⌘Home / ⌘End", linux: "Ctrl+Shift+Home / End", label: "Start / end of the scrollback" },
      { mac: "⌘-click", linux: "Ctrl-click", label: "Open a file path (src/app.ts:42) in your editor" },
      { mac: "⌘C", linux: "Edit → Copy", label: "Copy (lines a program wrapped come out as one)" },
      { mac: "⌘⇧C", linux: "Ctrl+Shift+Alt+C", label: "Copy exactly as displayed" },
      { mac: "⌘K", linux: "Ctrl+Shift+K", label: "Clear the screen and scrollback" },
    ],
  },
  {
    title: "Window",
    items: [
      { mac: "⌘N", linux: "Ctrl+Shift+N", label: "New window" },
      { mac: "⌘B", linux: "Ctrl+Shift+B", label: "Show / hide the sidebar" },
      { mac: "⌘= / ⌘- / ⌘0", linux: "Ctrl+Shift+= / - / 0", label: "Bigger / smaller / default font" },
      { mac: "⌘⇧R", linux: "Ctrl+Shift+Alt+R", label: "Redraw terminals (keeps scrollback)" },
      { mac: "⌘?", linux: "Ctrl+Shift+Alt+/", label: "This list of shortcuts" },
      { mac: "Esc", linux: "Esc", label: "Close a dialog / cancel rename" },
    ],
  },
];
