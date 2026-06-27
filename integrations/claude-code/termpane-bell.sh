#!/bin/sh
# TermPane <-> Claude Code bridge.
#
# Rings the terminal bell (BEL, \a) on TermPane's PTY so TermPane's attention
# indicator (pulsing dot + sidebar highlight, and an OS notification when the
# window is in the background) lights up when Claude Code finishes a turn or is
# waiting for your input.
#
# Wire it into Claude Code's Stop and Notification hooks (see the README in this
# directory). Claude Code does not emit a terminal bell on its own — it notifies
# via the OS — so this hook supplies the in-terminal signal TermPane can see.
#
# Why not `printf '\a' > /dev/tty`: Claude Code runs hooks detached from the
# controlling terminal, so /dev/tty is "device not configured". Instead we walk
# up the process tree to find an ancestor (the Claude process / its shell) that
# still has a controlling tty, and write the bell straight to that device.
#
# Scoped to TermPane via TERM_PROGRAM so your other terminals don't audibly beep.

[ "$TERM_PROGRAM" = "TermPane" ] || exit 0

p=$PPID
i=0
while [ "$i" -lt 6 ]; do
  [ -z "$p" ] && break
  [ "$p" = "0" ] && break
  t=$(ps -o tty= -p "$p" 2>/dev/null | tr -d ' ')
  case "$t" in
    # macOS reports the controlling tty as ttys###; Linux as pts/#.
    ttys*|pts/*)
      printf '\a' > "/dev/$t" 2>/dev/null
      exit 0
      ;;
  esac
  p=$(ps -o ppid= -p "$p" 2>/dev/null | tr -d ' ')
  i=$((i + 1))
done
exit 0
