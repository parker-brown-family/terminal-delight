/* ============================================================
   panes.js — content factories for leaf panes.
   Each returns { el, title, focus }. el is cached & reattached across
   re-renders so terminal scrollback / input survive splits & moves.
   ============================================================ */

let seq = 0;
const uid = () => `pane-${++seq}`;

/* ---------------- faux terminal ---------------- */
const BANNER = [
  ['', ''],
  ['  terminal-delight', 'ok'],
  ['  a tilix-shaped web workspace · type ', 'muted'],
  ['help', 'ok'],
  ['  for commands.', 'muted'],
  ['', ''],
];

function createTerminal(label) {
  const el = document.createElement('div');
  el.className = 'term';
  el.tabIndex = 0;

  const out = document.createElement('div');
  const row = document.createElement('div');
  row.className = 'term-input-row';
  const pfx = document.createElement('span');
  pfx.className = 'pfx';
  pfx.textContent = '$';
  const input = document.createElement('input');
  input.className = 'term-input';
  input.setAttribute('aria-label', 'terminal input');
  input.autocapitalize = 'off'; input.autocomplete = 'off'; input.spellcheck = false;
  row.append(pfx, input);
  el.append(out, row);

  const print = (text, cls = '') => {
    const ln = document.createElement('div');
    ln.className = 'ln' + (cls ? ' ' + cls : '');
    ln.textContent = text;
    out.appendChild(ln);
    el.scrollTop = el.scrollHeight;
    return ln;
  };
  const printPrompt = (cmd) => {
    const ln = document.createElement('div');
    ln.className = 'ln';
    const s = document.createElement('span'); s.className = 'pfx'; s.textContent = '$ ';
    ln.append(s, document.createTextNode(cmd));
    out.appendChild(ln);
  };

  BANNER.forEach(([t, c]) => {
    if (t === 'help') { const last = out.lastChild; if (last) { const s = document.createElement('span'); s.className = 'ok'; s.textContent = 'help'; last.appendChild(s); } }
    else print(t, c);
  });

  const COMMANDS = {
    help: () => {
      print('available:', 'muted');
      print('  help · clear · ls · whoami · theme · echo <text> · date · neofetch', 'ok');
    },
    clear: () => { out.innerHTML = ''; },
    ls: () => print('dashboard  projects  board  tasks  triage  schedule  assistant', 'ok'),
    whoami: () => print('parker @ terminal-delight (parker-brown-family)', 'ok'),
    theme: () => print('use the ◉ badge, top-right — 4 themes × any seed colour. Switches are instant (CSS vars, no re-render).', 'muted'),
    date: () => print(new Date().toString(), 'muted'),
    neofetch: () => {
      print('terminal-delight', 'ok');
      print('-----------------', 'muted');
      print('chrome    tabs · splits · drag-divider · break-out', 'muted');
      print('theme     hacker / tactical-overdrive / field-command / quiet-command', 'muted');
      print('engine    seed-hue → HSL palette → CSS custom props', 'muted');
      print('render    compositor-only effects ⇒ tilix-class snappiness', 'muted');
    },
  };

  const run = (raw) => {
    const cmd = raw.trim();
    printPrompt(cmd);
    if (!cmd) return;
    const [name, ...rest] = cmd.split(/\s+/);
    if (name === 'echo') return void print(rest.join(' '));
    const fn = COMMANDS[name];
    if (fn) fn();
    else print(`command not found: ${name}`, 'err');
  };

  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { const v = input.value; input.value = ''; run(v); }
  });
  el.addEventListener('mousedown', () => setTimeout(() => input.focus(), 0));

  return { el, title: label || 'shell', focus: () => input.focus() };
}

/* ---------------- mock PM panel (echoes the imt look) ---------------- */
function createPanel() {
  const el = document.createElement('div');
  el.className = 'panel-pad';
  el.innerHTML = `
    <h2>IMT PM Tool — Team-Usable Upgrade</h2>
    <p>Software · In Progress · owned by ryan.g@test.com</p>
    <dl class="kv">
      <dt>type</dt><dd>Software</dd>
      <dt>status</dt><dd>In Progress</dd>
      <dt>risk</dt><dd>Low</dd>
      <dt>progress</dt><dd>4/15 (27%)</dd>
      <dt>due</dt><dd>25 Mar 2026</dd>
      <dt>priority</dt><dd>P0</dd>
    </dl>`;
  return { el, title: 'project', focus: () => {} };
}

/* ---------------- mock AI assistant ---------------- */
function createAssistant() {
  const el = document.createElement('div');
  el.className = 'panel-pad';
  el.innerHTML = `
    <h2>PM ASSISTANT // SUB-TERMINAL</h2>
    <p>Session context: theming initiative — make the PM tool a delight to live in.</p>
    <dl class="kv">
      <dt>session</dt><dd>aeef4b4c-eb99</dd>
      <dt>recent</dt><dd>4 palettes wired</dd>
      <dt>branch</dt><dd>terminal-delight/main</dd>
      <dt>task</dt><dd>Tilix chrome + theme engine</dd>
    </dl>`;
  return { el, title: 'assistant', focus: () => {} };
}

const FACTORIES = { terminal: createTerminal, panel: createPanel, assistant: createAssistant };

export function makeContent(paneType = 'terminal', label) {
  const make = FACTORIES[paneType] || createTerminal;
  const c = make(label);
  return { id: uid(), paneType, ...c };
}
