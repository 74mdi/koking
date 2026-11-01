/**
 * Context Overflow Handoff.
 *
 * When a turn ends in error because the request exceeded the model context
 * window (or the provider wrapped it as EAC_UPSTREAM_FAILED), export the whole
 * conversation to a file, open a fresh chat in the same workspace, and ask it
 * to continue the interrupted work from that export.
 *
 * Deliberately dependency-free: only `ctx` services and node builtins, so it
 * mounts in any profile that has the session controller.
 *
 * @module index.js
 */

import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { randomUUID } from 'node:crypto'

/** Failure codes that mean "this request was too big for the model". */
const OVERFLOW_CODES = new Set(['CONTEXT_WINDOW_EXCEEDED', 'EAC_UPSTREAM_FAILED'])

/** Provider wordings that mean the same thing when the code is generic. */
const OVERFLOW_TEXT = /maximum context length|context length|context window|reduce the length of the messages|prompt is too long|too many tokens|token limit exceeded|request_too_large/i

const DEFAULT_PROMPT = [
  'A previous chat hit the model context limit and could not continue.',
  '',
  'Exported transcript: {export}',
  'Interrupted chat: {session}',
  '',
  'Continue that work from where it stopped. Read the export when you need the earlier detail; do not repeat finished steps.',
].join('\n')

/**
 * Whether one LlmFailure describes a request that outgrew the context window.
 * @param failure - `turn/end` reason.error, verbatim.
 * @returns true when the turn should hand off to a fresh chat.
 */
export function isOverflowFailure(failure) {
  if (failure === null || typeof failure !== 'object') return false
  if (typeof failure.code === 'string' && OVERFLOW_CODES.has(failure.code)) return true
  return typeof failure.message === 'string' && OVERFLOW_TEXT.test(failure.message)
}

/** Bound one rendered block so the export stays readable. */
function clip(text, max) {
  const value = typeof text === 'string' ? text : String(text ?? '')
  return value.length <= max ? value : `${value.slice(0, max)}…`
}

/** Render one message's content blocks as plain markdown. */
function renderBlocks(blocks) {
  const parts = []
  for (const block of Array.isArray(blocks) ? blocks : []) {
    switch (block?.type) {
      case 'text':
        parts.push(clip(block.text, 4000))
        break
      case 'reasoning':
        parts.push(`_(reasoning)_ ${clip(block.text, 800)}`)
        break
      case 'tool-call':
        parts.push(`→ tool \`${block.name}\` ${clip(block.arguments, 400)}`)
        break
      case 'tool-addition':
        parts.push(`_(tool available: ${block.toolName})_`)
        break
      case 'tool-removal':
        parts.push(`_(tool removed: ${block.toolName})_`)
        break
      case 'image':
        parts.push('_(image)_')
        break
      case 'file':
        parts.push(`_(file: ${block.attachment?.name ?? 'attachment'})_`)
        break
      default:
        break
    }
  }
  return parts.join('\n\n')
}

/** Render the conversation this session currently exposes, plus its lineage. */
function renderTranscript(session, reason, turn) {
  const lines = [
    `# Context handoff export`,
    '',
    `- session: \`${session.id}\``,
    `- cwd: \`${session.header.cwd ?? ''}\``,
    `- exported: ${new Date().toISOString()}`,
    `- failed turn: ${turn ?? '?'}`,
    `- failure: \`${reason.error?.code ?? 'UNKNOWN'}\` ${reason.error?.message ?? ''}`,
    '',
    '## Conversation',
    '',
  ]
  let messages = []
  try {
    messages = session.deriveMessages()
  } catch {
    messages = []
  }
  for (const message of messages) {
    if (message.role !== 'user' && message.role !== 'assistant' && message.role !== 'tool') continue
    const body = renderBlocks(message.content)
    if (body.trim() === '') continue
    lines.push(`### ${message.role}`, '', body, '')
  }
  return `${lines.join('\n')}\n`
}

/** Last user text, used to restate the interrupted request in the new chat. */
function lastUserText(session) {
  let messages = []
  try {
    messages = session.deriveMessages()
  } catch {
    return ''
  }
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index]
    if (message.role !== 'user') continue
    const text = renderBlocks(message.content)
    if (text.trim() !== '') return clip(text, 2000)
  }
  return ''
}

/** Resolve the workspace that owns a session, when the profile has one. */
function workspaceFor(ctx, sessionId) {
  const registry = ctx.get('workspaceRegistry')
  if (registry === undefined) return undefined
  try {
    return registry.list().find((workspace) => workspace.sessionIds?.includes(sessionId))
  } catch {
    return undefined
  }
}

/**
 * @param ctx - Host context with the session controller and filesystem.
 * @param config - `exportDir`, `continuePrompt`, `maxHandoffsPerSession`.
 */
export function apply(ctx, config = {}) {
  const exportDir = typeof config.exportDir === 'string' && config.exportDir !== '' ? config.exportDir : '.dsh/handoffs'
  const promptTemplate = typeof config.continuePrompt === 'string' && config.continuePrompt !== '' ? config.continuePrompt : DEFAULT_PROMPT
  const maxHandoffs = Number.isInteger(config.maxHandoffsPerSession) && config.maxHandoffsPerSession > 0 ? config.maxHandoffsPerSession : 1

  /** Sessions this plugin opened; they never hand off again (no chains). */
  const spawned = new Set()
  /** Per-source handoff counters, so one chat cannot fan out. */
  const counts = new Map()
  /** In-flight handoffs, awaited on unload. */
  const pending = new Set()

  ctx.effect(() => async () => {
    await Promise.allSettled([...pending])
  }, 'context-overflow-handoff.drain')

  const handoff = async (session, reason, turn) => {
    const cwd = session.header.cwd ?? process.cwd()
    const dir = path.resolve(cwd, exportDir)
    let file = null
    try {
      await mkdir(dir, { recursive: true })
      file = path.join(dir, `handoff-${session.id}-${Date.now()}.md`)
      await writeFile(file, renderTranscript(session, reason, turn), 'utf8')
    } catch (error) {
      ctx.logger.warn(`context-overflow-handoff: could not export the log: ${error instanceof Error ? error.message : String(error)}`)
      file = null
    }

    const controller = ctx.get('sessionController')
    if (controller === undefined) {
      ctx.logger.warn(`context-overflow-handoff: sessionController is absent; no continuation chat`)
      return
    }

    const workspace = workspaceFor(ctx, session.id)
    const created = await controller.create({
      ...workspace === undefined ? { cwd } : { workspaceId: workspace.id },
      ...session.header.agentPreset === undefined ? {} : { agentPreset: session.header.agentPreset },
    })
    spawned.add(created.sessionId)

    const previous = lastUserText(session)
    const text = [
      promptTemplate.replaceAll('{export}', file ?? '(export failed)').replaceAll('{session}', session.id),
      previous === '' ? '' : `\nThe interrupted request was:\n\n${previous}`,
    ].join('\n')

    await controller.prompt({
      requestId: randomUUID(),
      sessionId: created.sessionId,
      mode: 'queue',
      content: [{ type: 'text', text }],
    }, new AbortController().signal)

    ctx.logger.info(`context-overflow-handoff: ${file === null ? 'log export failed' : `exported ${file}`}; continuing in ${created.sessionId}`)
  }

  ctx.on('session/event', (session, event) => {
    if (event.type !== 'turn/end') return
    const reason = event.data?.reason
    if (reason?.kind !== 'error' || !isOverflowFailure(reason.error)) return
    if (session.header.origin === 'subagent' || spawned.has(session.id)) return
    const used = counts.get(session.id) ?? 0
    if (used >= maxHandoffs) return
    counts.set(session.id, used + 1)

    const task = handoff(session, reason, event.data?.turn)
      .catch((error) => {
        ctx.logger.warn(`context-overflow-handoff: handoff for "${session.id}" failed: ${error instanceof Error ? error.message : String(error)}`)
      })
      .finally(() => {
        pending.delete(task)
      })
    pending.add(task)
  })
}
