/* popout.js — standalone window hosting one broken-out pane. */
import { initTheme } from './theme-engine.js';
import { makeContent } from './panes.js';

initTheme();

const q = new URLSearchParams(location.search);
const paneType = q.get('type') || 'terminal';
const title = q.get('title') || 'pane';
document.title = `▸ ${title} — terminal-delight`;

const c = makeContent(paneType, title);
const head = document.getElementById('pop-title');
if (head) head.textContent = title;
const body = document.getElementById('pop-body');
if (body) { body.appendChild(c.el); c.focus && c.focus(); }
