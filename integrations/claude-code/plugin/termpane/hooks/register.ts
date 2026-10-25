import type { EngineInterface, Register } from 'claude-code'

// TERMPANE_BIN is set by TermPane in every shell it spawns: the path of its
// own binary, whose `hook`, `tool` and `fork` subcommands talk to the TermPane
// window the shell lives in. Outside TermPane it is unset and this plugin stays
// inert — no status forwarding, no tools, and /fork keeps its built-in behavior.

// Tools served by `termpane tool <name>`; their descriptions and schemas come
// from the binary itself (`termpane tool --list`) so they can't drift.
const TOOLS = ['list_tabs', 'read_output', 'send_input', 'open_tab', 'focus_tab', 'close_tab'] as const

type ToolSpec = { name: string; description: string; inputSchema?: Record<string, unknown> }

// Hands one hook payload to TermPane. Never throws and never blocks for long:
// status reporting must not get in the way of the session.
async function forward($: EngineInterface, payload: unknown): Promise<void> {
  const bin = await $.env.get('TERMPANE_BIN')
  if (!bin) return
  try {
    await $.process.run([bin, 'hook'], { stdin: JSON.stringify(payload), timeoutMs: 3000 })
  } catch {
    // TermPane closed or busy; the next event will catch up.
  }
}

// TermPane reads the settings hooks' payload shape (`hook_event_name`,
// `session_id`, `cwd`, `tool_name`, `tool_input`, `message`); build it from
// the function-hook events. These are used rather than `classic.*`, which a
// plugin seated above this one (e.g. an organization's managed plugin) may
// pass on without running the hooks beneath it.
async function report($: EngineInterface, event: string, fields: Record<string, unknown> = {}) {
  await forward($, {
    hook_event_name: event,
    session_id: await $.session.id(),
    cwd: await $.session.cwd(),
    ...fields,
  })
}

// A tool call's own arguments, without the keys the engine adds beside them.
function toolArgs(e: unknown): Record<string, unknown> {
  const { tool: _t, tool_use_id: _id, agentId: _a, requestMeta: _m, consent: _c, ...args } =
    e as Record<string, unknown>
  return args
}

// What TermPane shows beyond status: work Claude left running in the
// background (the spy button) and artifacts it published (the artifacts list).
async function reportOutcome(
  $: EngineInterface,
  tool: string,
  input: Record<string, unknown>,
  result: unknown,
) {
  const r = (result ?? {}) as Record<string, unknown>
  const str = (v: unknown) => (typeof v === 'string' ? v : undefined)
  if (tool === 'Bash' && str(r.backgroundTaskId)) {
    const command = str(input.command) ?? ''
    await report($, 'TermPaneBackground', {
      task: {
        id: r.backgroundTaskId,
        kind: 'bash',
        description: str(input.description) ?? command.slice(0, 80),
        outputFile: str(r.backgroundOutputPath),
      },
    })
  } else if ((tool === 'Agent' || tool === 'Task') && r.status === 'async_launched') {
    await report($, 'TermPaneBackground', {
      task: {
        id: r.agentId,
        kind: 'agent',
        description: str(r.description) ?? 'Background agent',
        outputFile: str(r.outputFile),
      },
    })
  } else if (tool === 'Artifact' && str(r.url) && (str(input.action) ?? 'publish') === 'publish') {
    const file = str(input.file_path)
    await report($, 'TermPaneArtifact', {
      artifact: {
        url: r.url,
        title: str(r.title) ?? str(input.title) ?? (file ? file.split('/').pop() : undefined),
      },
    })
  }
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    const bin = await $.env.get('TERMPANE_BIN')
    if (bin) {
      try {
        const listed = await $.process.run([bin, 'tool', '--list'], { timeoutMs: 5000 })
        const specs = JSON.parse(listed.stdout) as ToolSpec[]
        for (const spec of specs) {
          if ((TOOLS as readonly string[]).includes(spec.name)) await $.tool.register(spec)
        }
      } catch {
        // An older TermPane without `tool --list`: no tools this session.
      }
    }
    const started = await next(e)
    await report($, 'SessionStart')
    return started
  })
  on('session.end', async ($, e, next) => {
    await forward($, {
      hook_event_name: 'SessionEnd',
      session_id: e.sessionId,
      cwd: await $.session.cwd(),
      reason: e.reason,
    })
    return next(e)
  })

  // The main loop's turns only: a subagent's turn ending is not Claude done.
  on('turn.start', async ($, e, next) => {
    if (!e.agentId) await report($, 'UserPromptSubmit')
    return next(e)
  })
  on('turn.complete', async ($, e, next) => {
    if (!(e as { agentId?: string }).agentId) await report($, 'Stop')
    return next(e)
  })

  on('tool.call', async ($, e, next) => {
    const tool_input = toolArgs(e)
    await report($, 'PreToolUse', { tool_name: e.tool, tool_input })
    // A question Claude puts to the user always waits for them, in any mode.
    if (e.tool === 'AskUserQuestion' && !e.agentId) {
      await report($, 'Notification', {
        notification_type: 'question',
        message: 'Claude is asking you a question',
      })
    }
    const ran = await next(e)
    await report($, 'PostToolUse', { tool_name: e.tool, tool_input })
    await reportOutcome($, e.tool, tool_input, ran.result)
    return ran
  })

  // An `ask` put to a person may be the permission dialog. In auto mode most
  // asks go to Claude's classifier instead and never show one; those that
  // still do come from an ask rule, a hook or a setting (a read outside the
  // working folders) — `forced` tells TermPane, which knows the session's
  // mode from its transcript and drops the rest. Queries (no tool_use_id)
  // decide nothing and are ignored.
  on('tool.check', async ($, e, next) => {
    const verdict = await next(e)
    if (verdict.decision === 'ask' && e.approver === 'person' && e.tool_use_id) {
      await report($, 'Notification', {
        notification_type: 'permission_prompt',
        message: `Claude needs your permission to use ${e.tool}`,
        forced: Boolean(verdict.rule || verdict.hook || verdict.setting),
      })
    }
    return verdict
  })

  // Tab control tools, listed as mcp__termpane__<name>.
  for (const name of TOOLS) {
    on('tool.call', { tool: `mcp__termpane__${name}` }, async ($, e) => {
      const bin = await $.env.get('TERMPANE_BIN')
      if (!bin) return { deny: 'This session is not running inside TermPane.' }
      const ran = await $.process.run([bin, 'tool', name, JSON.stringify(toolArgs(e))], {
        timeoutMs: 30000,
      })
      const text = (ran.stdout || ran.stderr).trim()
      return ran.exitCode === 0 ? { result: text } : { deny: text || `${name} failed` }
    })
  }

  // /fork inside TermPane: copy this session into a TermPane sub-tab nested
  // under the current tab ("FORK - <tab>"), running `claude --resume <id>
  // --fork-session`. Outside TermPane, or if TermPane can't do it, the built-in
  // /fork runs as usual.
  on('command.run', { command: 'fork' }, async ($, e, next) => {
    const bin = await $.env.get('TERMPANE_BIN')
    if (!bin) return next(e)
    const ran = await $.process.run(
      [bin, 'fork', '--session', await $.session.id(), '--prompt', e.args],
      { timeoutMs: 10000 },
    )
    if (ran.exitCode !== 0) return next(e)
    return { text: ran.stdout.trim() }
  }).catch(($, e, next) => next(e))
}
