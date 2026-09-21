// Tests for scripts/td-agent-hooks — the Claude Code hook adapter for the agent
// channel (TDAC 0.1). Run with:  node --test scripts/td-agent-hooks.test.mjs
//
// Every case drives the real script with a real payload on stdin and reads the
// journal it wrote, in a scratch XDG_STATE_HOME. The waiting case writes the
// answer file from the test while the hook is polling, the way the window does.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, 'td-agent-hooks');

function scratch() {
  const state = mkdtempSync(join(tmpdir(), 'tdac-'));
  const env = { PATH: process.env.PATH, HOME: process.env.HOME, XDG_STATE_HOME: state, TD_SESSION: 'attention', TD_PANE_ID: '7' };
  const dir = join(state, 'terminal-delight', 'surfaces', 'attention', '7');
  return { state, env, dir };
}

function run(payload, env, extra = {}) {
  const started = Date.now();
  const out = spawnSync('bash', [script], {
    input: typeof payload === 'string' ? payload : JSON.stringify(payload),
    env: { ...env, ...extra },
    encoding: 'utf8',
    timeout: 20000,
  });
  return { ...out, ms: Date.now() - started };
}

function lines(dir) {
  const p = join(dir, 'inbound.jsonl');
  if (!existsSync(p)) return [];
  return readFileSync(p, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
}

const question = {
  session_id: 's1',
  hook_event_name: 'PreToolUse',
  tool_name: 'AskUserQuestion',
  tool_use_id: 'toolu_01ABC',
  tool_input: {
    questions: [
      { question: 'Which drink?', header: 'Drink', multiSelect: false,
        options: [{ label: 'Tea', description: 'leaves' }, { label: 'Coffee', description: 'beans', preview: 'a long preview' }] },
    ],
  },
};

test('outside a Terminal Delight pane the hook does nothing at all', () => {
  const { env, dir } = scratch();
  const bare = { PATH: env.PATH, HOME: env.HOME, XDG_STATE_HOME: env.XDG_STATE_HOME };
  const r = run({ hook_event_name: 'UserPromptSubmit', prompt: 'hi' }, bare);
  assert.equal(r.status, 0);
  assert.equal(r.stdout, '');
  assert.equal(lines(dir).length, 0);
});

test('a prompt, a reply and a notification each become one record', () => {
  const { env, dir } = scratch();
  assert.equal(run({ session_id: 's1', hook_event_name: 'UserPromptSubmit', prompt_id: 'p1', prompt: 'fix the bug' }, env).status, 0);
  assert.equal(run({ session_id: 's1', hook_event_name: 'Stop', last_assistant_message: 'done' }, env).status, 0);
  assert.equal(run({ session_id: 's1', hook_event_name: 'Notification', notification_type: 'idle_prompt', message: 'waiting' }, env).status, 0);
  // A reply the harness did not hand over is null, not an empty string.
  assert.equal(run({ session_id: 's1', hook_event_name: 'Stop' }, env).status, 0);
  const got = lines(dir);
  assert.equal(got.length, 4);
  assert.equal(got[0].type, 'prompt');
  assert.equal(got[0].text, 'fix the bug');
  assert.equal(got[0].prompt_id, 'p1');
  assert.equal(got[0].td, '0.2');
  assert.equal(typeof got[0].at_ms, 'number');
  assert.equal(got[1].type, 'reply');
  assert.equal(got[1].text, 'done');
  assert.equal(got[2].type, 'notify');
  assert.equal(got[2].notification_type, 'idle_prompt');
  assert.equal(got[3].type, 'reply');
  assert.equal(got[3].text, null);
});

test('a question with no bench open is recorded whole and released at once', () => {
  const { env, dir } = scratch();
  const r = run(question, env);
  assert.equal(r.status, 0);
  assert.equal(r.stdout, '', 'no pre-answer: the picker paints as before');
  assert.ok(r.ms < 3000, `returned at once, not after a wait: ${r.ms}ms`);
  const got = lines(dir);
  assert.equal(got.length, 2);
  assert.equal(got[0].type, 'question');
  assert.equal(got[0].tool_use_id, 'toolu_01ABC');
  assert.deepEqual(got[0].questions, question.tool_input.questions, 'verbatim, previews included');
  assert.equal(typeof got[0].deadline_ms, 'number');
  assert.equal(got[1].type, 'released');
  assert.equal(got[1].why, 'missing', 'no marker at all: no window of this build has the pane');
  assert.equal(typeof got[1].at_ms, 'number', 'a release says when');
});

test('a bench that closes mid-wait releases the picker as closed, not stale', async () => {
  const { env, dir } = scratch();
  mkdirSync(dir, { recursive: true });
  const write = (face) => writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.2', face, at_ms: Date.now(), window: 1 }));
  write('workbench');
  const started = Date.now();
  const child = spawn('bash', [script], { env, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = '';
  child.stdout.on('data', (d) => { stdout += d; });
  child.stdin.end(JSON.stringify(question));
  // The person flips the pane back to the terminal face (or switches tabs):
  // the window rewrites the marker with a fresh clock and the other face.
  const keep = setInterval(() => write(Date.now() - started > 600 ? 'terminal' : 'workbench'), 200);
  const code = await new Promise((r) => child.on('close', r));
  clearInterval(keep);
  assert.equal(code, 0);
  assert.equal(stdout, '');
  const took = Date.now() - started;
  assert.ok(took < 3500, `released on the next marker read, not at the age limit: ${took}ms`);
  const last = lines(dir).at(-1);
  assert.equal(last.type, 'released');
  assert.equal(last.why, 'closed');
});

test('the journal rotates at the cap, under the same lock the appends take', () => {
  const { env, dir } = scratch();
  mkdirSync(dir, { recursive: true });
  const p = join(dir, 'inbound.jsonl');
  // A journal already past a tiny cap.
  writeFileSync(p, JSON.stringify({ td: '0.2', type: 'reply', text: 'x'.repeat(600) }) + '\n');
  const r = run({ session_id: 's1', hook_event_name: 'UserPromptSubmit', prompt: 'after the roll' }, env, { TD_INBOUND_CAP: '500' });
  assert.equal(r.status, 0);
  assert.ok(existsSync(p + '.1'), 'the old journal moved aside');
  const now = lines(dir);
  assert.equal(now.length, 1, 'the new journal holds only the new record');
  assert.equal(now[0].text, 'after the roll');
  const old = readFileSync(p + '.1', 'utf8').trim().split('\n');
  assert.equal(old.length, 1);
});

test('with a bench open the hook waits, and the bench\'s answer becomes the pre-answer', async () => {
  const { env, dir } = scratch();
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'workbench', at_ms: Date.now(), window: 1 }));
  const child = spawn('bash', [script], { env, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = '';
  child.stdout.on('data', (d) => { stdout += d; });
  child.stdin.end(JSON.stringify(question));
  // Keep the marker fresh while the window "thinks", then answer.
  const keepalive = setInterval(() => {
    writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'workbench', at_ms: Date.now(), window: 1 }));
  }, 500);
  await new Promise((r) => setTimeout(r, 900));
  mkdirSync(join(dir, 'answers'), { recursive: true });
  writeFileSync(join(dir, 'answers', 'toolu_01ABC.json'), JSON.stringify({ td: '0.1', tool_use_id: 'toolu_01ABC', answers: { 'Which drink?': 'Coffee' } }));
  const code = await new Promise((r) => child.on('close', r));
  clearInterval(keepalive);
  assert.equal(code, 0);
  const decision = JSON.parse(stdout.trim());
  assert.equal(decision.hookSpecificOutput.hookEventName, 'PreToolUse');
  assert.equal(decision.hookSpecificOutput.permissionDecision, 'allow');
  assert.deepEqual(decision.hookSpecificOutput.updatedInput.answers, { 'Which drink?': 'Coffee' });
  assert.deepEqual(decision.hookSpecificOutput.updatedInput.questions, question.tool_input.questions, 'the questions ride along untouched');
  const types = lines(dir).map((l) => l.type + (l.why ? ':' + l.why : ''));
  assert.deepEqual(types, ['question', 'waiting', 'released:answered']);
});

test('a marker that goes stale mid-wait releases the picker within seconds', async () => {
  const { env, dir } = scratch();
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'workbench', at_ms: Date.now(), window: 1 }));
  const started = Date.now();
  const child = spawn('bash', [script], { env, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = '';
  child.stdout.on('data', (d) => { stdout += d; });
  child.stdin.end(JSON.stringify(question));
  // The window dies: the marker is never refreshed again.
  const code = await new Promise((r) => child.on('close', r));
  assert.equal(code, 0);
  assert.equal(stdout, '');
  const took = Date.now() - started;
  assert.ok(took >= 3500 && took < 9000, `released when the marker aged out, not at the deadline: ${took}ms`);
  const last = lines(dir).at(-1);
  assert.equal(last.type, 'released');
  assert.equal(last.why, 'stale');
});

test('the wait is bounded by TD_ASK_WAIT_S even while the marker stays fresh', async () => {
  const { env, dir } = scratch();
  mkdirSync(dir, { recursive: true });
  const keepalive = setInterval(() => {
    writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'workbench', at_ms: Date.now(), window: 1 }));
  }, 500);
  writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'workbench', at_ms: Date.now(), window: 1 }));
  const r = run(question, env, { TD_ASK_WAIT_S: '1' });
  clearInterval(keepalive);
  assert.equal(r.status, 0);
  assert.equal(r.stdout, '');
  assert.ok(r.ms >= 900 && r.ms < 4000, `${r.ms}ms`);
  const last = lines(dir).at(-1);
  assert.equal(last.why, 'timeout');
});

test('a terminal-face marker is not a bench, and neither is a marker from the future', () => {
  const { env, dir } = scratch();
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'terminal', at_ms: Date.now(), window: 1 }));
  assert.equal(lines(dir).length, 0);
  let r = run(question, env);
  assert.equal(lines(dir).at(-1).why, 'closed');
  assert.ok(r.ms < 3000);
  writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'workbench', at_ms: Date.now() + 60_000, window: 1 }));
  r = run(question, env);
  assert.equal(lines(dir).at(-1).why, 'stale');
  assert.ok(r.ms < 3000);
});

test('the answer file is named by the same filter the window applies', async () => {
  const { env, dir } = scratch();
  mkdirSync(join(dir, 'answers'), { recursive: true });
  writeFileSync(join(dir, 'bench.json'), JSON.stringify({ td: '0.1', face: 'workbench', at_ms: Date.now(), window: 1 }));
  const hostile = { ...question, tool_use_id: '../../etc/passwd' };
  const child = spawn('bash', [script], { env, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = '';
  child.stdout.on('data', (d) => { stdout += d; });
  child.stdin.end(JSON.stringify(hostile));
  await new Promise((r) => setTimeout(r, 600));
  writeFileSync(join(dir, 'answers', 'etcpasswd.json'), JSON.stringify({ answers: { 'Which drink?': 'Tea' } }));
  const code = await new Promise((r) => child.on('close', r));
  assert.equal(code, 0);
  assert.deepEqual(JSON.parse(stdout).hookSpecificOutput.updatedInput.answers, { 'Which drink?': 'Tea' });
});

test('the harness\'s own record of the answer is forwarded whole', () => {
  const { env, dir } = scratch();
  const r = run({ session_id: 's1', hook_event_name: 'PostToolUse', tool_name: 'AskUserQuestion', tool_use_id: 'toolu_01ABC',
    tool_input: question.tool_input, tool_response: { questions: question.tool_input.questions, answers: { 'Which drink?': 'Tea' } } }, env);
  assert.equal(r.status, 0);
  const got = lines(dir);
  assert.equal(got.length, 1);
  assert.equal(got[0].type, 'answered');
  assert.equal(got[0].tool_use_id, 'toolu_01ABC');
  assert.deepEqual(got[0].answers.answers, { 'Which drink?': 'Tea' });
  // Another tool's PostToolUse is not our business.
  const other = run({ session_id: 's1', hook_event_name: 'PostToolUse', tool_name: 'Bash', tool_use_id: 'x', tool_response: {} }, env);
  assert.equal(other.status, 0);
  assert.equal(lines(dir).length, 1);
});

test('garbage on stdin exits 0 and writes nothing', () => {
  const { env, dir } = scratch();
  const r = run('{"hook_event_name": "Sto', env);
  assert.equal(r.status, 0);
  assert.equal(r.stdout, '');
  assert.equal(lines(dir).length, 0);
  const empty = run('', env);
  assert.equal(empty.status, 0);
});
