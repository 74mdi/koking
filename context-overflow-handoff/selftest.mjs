import assert from 'node:assert/strict'
import { mkdtemp, readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { apply, isOverflowFailure } from './index.js'

assert.equal(isOverflowFailure({ code: 'EAC_UPSTREAM_FAILED', message: 'boom' }), true)
assert.equal(isOverflowFailure({ code: 'CONTEXT_WINDOW_EXCEEDED', message: 'x' }), true)
assert.equal(
  isOverflowFailure({
    code: 'SERVER',
    message: "Your request resulted in more tokens than this model's maximum context length allows. Please reduce the length of the messages or run compaction.",
  }),
  true,
)
assert.equal(isOverflowFailure({ code: 'RATE_LIMIT', message: 'slow down' }), false)
assert.equal(isOverflowFailure(undefined), false)
assert.equal(isOverflowFailure(null), false)

const cwd = await mkdtemp(path.join(tmpdir(), 'handoff-'))
const listeners = {}
const calls = []
const effects = []

const ctx = {
  logger: { info() {}, warn() {} },
  get(name) {
    if (name === 'sessionController') {
      return {
        async create(request) {
          calls.push(['create', request])
          return { sessionId: 'session-new' }
        },
        async prompt(request) {
          calls.push(['prompt', request])
        },
      }
    }
    if (name === 'workspaceRegistry') {
      return { list: () => [{ id: 'ws-1', path: cwd, sessionIds: ['session-old'] }] }
    }
    return undefined
  },
  effect(factory) {
    effects.push(factory())
  },
  on(event, listener) {
    listeners[event] = listener
  },
}

apply(ctx, { exportDir: 'handoffs' })

const session = {
  id: 'session-old',
  header: { cwd, agentPreset: 'standard' },
  deriveMessages: () => [
    { role: 'user', content: [{ type: 'text', text: 'build the thing' }] },
    { role: 'assistant', content: [{ type: 'text', text: 'working on it' }] },
  ],
}

const reason = {
  kind: 'error',
  error: {
    code: 'EAC_UPSTREAM_FAILED',
    message: "Your request resulted in more tokens than this model's maximum context length allows. Please reduce the length of the messages or run compaction.",
  },
}

listeners['session/event'](session, { type: 'turn/end', data: { turn: 3, reason } })
await new Promise((resolve) => setTimeout(resolve, 50))

const created = calls.find(([name]) => name === 'create')
assert.ok(created, 'a continuation session was created')
assert.deepEqual(created[1], { workspaceId: 'ws-1', agentPreset: 'standard' })

const prompted = calls.find(([name]) => name === 'prompt')
assert.ok(prompted, 'the continuation session was prompted')
const text = prompted[1].content[0].text
assert.match(text, /handoff-session-old-\d+\.md/)
assert.match(text, /build the thing/)

const file = text.match(/handoff-session-old-\d+\.md/)[0]
const exported = await readFile(path.join(cwd, 'handoffs', file), 'utf8')
assert.match(exported, /### user/)
assert.match(exported, /build the thing/)
assert.match(exported, /EAC_UPSTREAM_FAILED/)

const before = calls.length
listeners['session/event'](session, { type: 'turn/end', data: { turn: 4, reason } })
await new Promise((resolve) => setTimeout(resolve, 20))
assert.equal(calls.length, before, 'a source session hands off only once')

for (const effect of effects) await effect()
console.log('context-overflow-handoff: ok')
