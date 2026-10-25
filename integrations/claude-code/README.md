# Claude Code notifications in TermPane

> **Superseded.** TermPane 0.4.0+ ships its own Claude Code plugin (in
> [`plugin/`](plugin/)): open **Preferences → Claude Code integration → Install
> Claude Code plugin**. It shows Claude's live status on each card, flags
> permission requests and finished turns, tracks the files each session
> changes, enables session restore, gives Claude tools to drive TermPane tabs,
> and makes `/fork` open the copy in a sub-tab. This bell script keeps working,
> but isn't needed anymore — you can remove its two hook entries.

TermPane lights up a terminal's sidebar card — a pulsing dot, a lingering
highlight, and (when the window is in the background) a native OS notification —
whenever a program **rings the terminal bell** (`\x07`). That works out of the
box for anything that rings the bell.

**Claude Code is a special case:** it doesn't ring the terminal bell on its own
— it notifies through the OS. So Claude finishing a turn never reaches TermPane.
This bridge fixes that: a tiny hook rings the bell on TermPane's terminal when
Claude stops or asks for input, so the card lights up like any other program.

## Setup

**1. Install the hook script** at `~/.claude/hooks/termpane-bell.sh`.

Copy [`termpane-bell.sh`](./termpane-bell.sh) there, or paste this self-contained
installer into a terminal:

```sh
mkdir -p ~/.claude/hooks
cat > ~/.claude/hooks/termpane-bell.sh <<'EOF'
#!/bin/sh
# Ring the terminal bell on TermPane's PTY (see TermPane integrations/claude-code).
[ "$TERM_PROGRAM" = "TermPane" ] || exit 0
p=$PPID
i=0
while [ "$i" -lt 6 ]; do
  [ -z "$p" ] && break
  [ "$p" = "0" ] && break
  t=$(ps -o tty= -p "$p" 2>/dev/null | tr -d ' ')
  case "$t" in
    ttys*|pts/*) printf '\a' > "/dev/$t" 2>/dev/null; exit 0 ;;
  esac
  p=$(ps -o ppid= -p "$p" 2>/dev/null | tr -d ' ')
  i=$((i + 1))
done
exit 0
EOF
chmod +x ~/.claude/hooks/termpane-bell.sh
```

**2. Wire it into Claude Code's hooks** in `~/.claude/settings.json`. Add the
command to the `Stop` hook (Claude finished a turn) and the `Notification` hook
(Claude needs permission / your input):

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command", "command": "sh ~/.claude/hooks/termpane-bell.sh" } ] }
    ],
    "Notification": [
      { "hooks": [ { "type": "command", "command": "sh ~/.claude/hooks/termpane-bell.sh" } ] }
    ]
  }
}
```

If you already have hooks (e.g. a sound), just add the command alongside them —
they all run:

```json
"Stop": [
  {
    "hooks": [
      { "type": "command", "command": "afplay /System/Library/Sounds/Glass.aiff", "async": true },
      { "type": "command", "command": "sh ~/.claude/hooks/termpane-bell.sh" }
    ]
  }
]
```

**3. Restart your Claude Code session** inside TermPane — hooks are read when a
session starts.

That's it. Switch away from the Claude terminal (another card, or another app),
let Claude finish, and the card lights up.

## Why the hook walks the process tree

Claude Code runs hooks detached from the controlling terminal, so the usual
`printf '\a' > /dev/tty` fails with *"device not configured."* The script walks
up to the Claude process, which still owns the PTY (e.g. `/dev/ttys004` on
macOS, `/dev/pts/4` on Linux), and writes the bell to that device directly. The
`TERM_PROGRAM = TermPane` guard keeps it from beeping in your other terminals.

## Simpler alternative (no script)

If you only ever use Claude Code inside TermPane, you can skip the hook and just
tell Claude to use the terminal bell as its notification channel — one line in
`~/.claude/settings.json`:

```json
{ "preferredNotifChannel": "terminal_bell" }
```

Claude then emits the bell itself, which TermPane catches. The trade-off: it's a
global Claude setting, so your **other** terminals (iTerm2, Terminal.app, …) will
also beep on every Claude notification. The hook above avoids that by being
scoped to TermPane.

## OSC notifications

TermPane also understands the `OSC 9` and `OSC 777` desktop-notification escape
sequences. Any tool that emits those — including Claude Code's `iterm2`
notification channel — lights up the card and forwards its message text to the
OS notification, with no hook required.
