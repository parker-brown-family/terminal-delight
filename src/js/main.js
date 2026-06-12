/* main.js — boot the theme engine, then build the initial workspace. */
import { initTheme } from './theme-engine.js';
import { Workspace } from './workspace.js';

initTheme();

const ws = new Workspace(
  document.getElementById('tabstrip'),
  document.getElementById('panes'),
);

/* Opening layout that shows the chrome off:
   tab 1  →  [ terminal | (project / assistant stacked) ]
   tab 2  →  single shell                                   */
const left = ws._make('terminal');
const right = {
  type: 'split', dir: 'col', sizes: [0.5, 0.5],
  children: [ws._make('panel'), ws._make('assistant')],
};
ws.addTab({ type: 'split', dir: 'row', sizes: [0.62, 0.38], children: [left, right] }, 'workspace', true);
ws.addTab(ws._make('terminal'), 'shell', false);
ws.renderTabs();

window.__ws = ws;   // handy for console poking
