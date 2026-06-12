/* ============================================================
   workspace.js — Tilix-shaped chrome.
   • tabs (add / close / drag-reorder)
   • binary tiling tree (split-right / split-down)
   • draggable splitters (live resize)
   • per-pane triple-button + close
   • drag a pane's header off the tiling area to break it out into a window

   The node tree only references leafIds; live content lives in contentMap so
   panes keep their state (terminal scrollback, input) across splits & moves.
   ============================================================ */
import { makeContent } from './panes.js';
import { detachPane } from './detach.js';

const ICON = {
  splitRight: '<svg viewBox="0 0 16 16"><rect x="1.5" y="2.5" width="13" height="11" rx="1.5"/><line x1="8" y1="2.5" x2="8" y2="13.5"/></svg>',
  splitDown:  '<svg viewBox="0 0 16 16"><rect x="1.5" y="2.5" width="13" height="11" rx="1.5"/><line x1="1.5" y1="8" x2="14.5" y2="8"/></svg>',
  detach:     '<svg viewBox="0 0 16 16"><rect x="1.5" y="4.5" width="9" height="9" rx="1.5"/><path d="M9 2.5h4.5V7"/><line x1="13.5" y1="2.5" x2="8" y2="8"/></svg>',
  close:      '<svg viewBox="0 0 16 16"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>',
};

export class Workspace {
  constructor(stripEl, panesEl) {
    this.strip = stripEl;
    this.panes = panesEl;
    this.tabs = [];
    this.active = null;
    this.content = new Map();      // leafId -> {id, el, paneType, title, focus}
    this.focusedLeaf = null;
    this.tabSeq = 0;
  }

  /* ---------- content helpers ---------- */
  _make(paneType, label) {
    const c = makeContent(paneType, label);
    this.content.set(c.id, c);
    return { type: 'leaf', leafId: c.id };
  }
  _leaf(node) { return { type: 'leaf', leafId: node.leafId }; }

  /* ---------- tab lifecycle ---------- */
  addTab(root, title, activate = true) {
    const id = `tab-${++this.tabSeq}`;
    const tab = { id, title: title || `shell ${this.tabSeq}`, root: root || this._make('terminal'), focusedLeafId: null };
    tab.focusedLeafId = firstLeaf(tab.root);
    this.tabs.push(tab);
    if (activate || !this.active) this.active = tab;
    this.renderTabs();
    if (activate) this.renderPanes();
    return tab;
  }
  closeTab(id) {
    const i = this.tabs.findIndex(t => t.id === id);
    if (i < 0) return;
    eachLeaf(this.tabs[i].root, lid => this.content.delete(lid));
    this.tabs.splice(i, 1);
    if (!this.tabs.length) { this.addTab(null, 'shell'); return; }
    if (this.active.id === id) this.active = this.tabs[Math.max(0, i - 1)];
    this.renderTabs(); this.renderPanes();
  }
  activateTab(id) {
    const t = this.tabs.find(t => t.id === id); if (!t) return;
    this.active = t; this.renderTabs(); this.renderPanes();
  }
  moveTab(from, to) {
    if (to < 0 || to >= this.tabs.length || from === to) return;
    const [t] = this.tabs.splice(from, 1);
    this.tabs.splice(to, 0, t);
    this.renderTabs();
  }

  /* ---------- tree ops ---------- */
  splitLeaf(leafId, dir) {
    const tab = this.active;
    const loc = locate(tab.root, n => n.type === 'leaf' && n.leafId === leafId);
    if (!loc) return;
    const fresh = this._make('terminal');
    const split = { type: 'split', dir, children: [loc.node, fresh], sizes: [0.5, 0.5] };
    if (!loc.parent) tab.root = split; else loc.parent.children[loc.index] = split;
    tab.focusedLeafId = fresh.leafId;
    this.renderPanes();
  }
  closeLeaf(leafId) {
    const tab = this.active;
    const loc = locate(tab.root, n => n.type === 'leaf' && n.leafId === leafId);
    if (!loc) return;
    this.content.delete(leafId);
    if (!loc.parent) { this.closeTab(tab.id); return; }       // last pane in tab
    const sibling = loc.parent.children[1 - loc.index];
    const gp = locate(tab.root, n => n === loc.parent);
    if (!gp.parent) tab.root = sibling; else gp.parent.children[gp.index] = sibling;
    if (tab.focusedLeafId === leafId) tab.focusedLeafId = firstLeaf(tab.root);
    this.renderPanes();
  }
  detachLeaf(leafId) {
    const c = this.content.get(leafId);
    if (c) detachPane(c.paneType, c.title);
    this.closeLeaf(leafId);
  }

  /* ---------- rendering ---------- */
  renderTabs() {
    this.strip.innerHTML = '';
    this.tabs.forEach((tab, idx) => {
      const el = document.createElement('div');
      el.className = 'tab' + (tab === this.active ? ' is-active' : '');
      el.draggable = true;
      el.innerHTML = `<span class="dot"></span><span class="label"></span><span class="x" title="close">×</span>`;
      el.querySelector('.label').textContent = tab.title;
      el.addEventListener('click', (e) => { if (!e.target.classList.contains('x')) this.activateTab(tab.id); });
      el.querySelector('.x').addEventListener('click', (e) => { e.stopPropagation(); this.closeTab(tab.id); });
      el.addEventListener('dblclick', (e) => { if (e.target.classList.contains('label')) this._rename(tab, e.target); });
      // drag-reorder
      el.addEventListener('dragstart', (e) => { e.dataTransfer.setData('text/tab', String(idx)); e.dataTransfer.effectAllowed = 'move'; });
      el.addEventListener('dragover', (e) => { if (e.dataTransfer.types.includes('text/tab')) { e.preventDefault(); el.classList.add('drag-over'); } });
      el.addEventListener('dragleave', () => el.classList.remove('drag-over'));
      el.addEventListener('drop', (e) => { e.preventDefault(); el.classList.remove('drag-over'); const from = Number(e.dataTransfer.getData('text/tab')); this.moveTab(from, idx); });
      this.strip.appendChild(el);
    });
    const add = document.createElement('button');
    add.className = 'tab-add'; add.title = 'new tab'; add.textContent = '+';
    add.addEventListener('click', () => this.addTab(null, `shell ${this.tabSeq + 1}`));
    this.strip.appendChild(add);
  }
  _rename(tab, labelEl) {
    const input = document.createElement('input');
    input.value = tab.title; input.className = 'term-input'; input.style.width = '120px';
    labelEl.replaceWith(input); input.focus(); input.select();
    const commit = () => { tab.title = input.value.trim() || tab.title; this.renderTabs(); };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') commit(); if (e.key === 'Escape') this.renderTabs(); });
    input.addEventListener('blur', commit);
  }

  renderPanes() {
    this.panes.innerHTML = '';
    this.panes.appendChild(this._renderNode(this.active.root));
    const c = this.content.get(this.active.focusedLeafId);
    if (c?.focus) c.focus();
  }
  _renderNode(node) {
    const el = document.createElement('div');
    el.className = 'node';
    if (node.type === 'leaf') { el.appendChild(this._buildPane(node.leafId)); return el; }
    el.classList.add('split', node.dir);
    const a = this._renderNode(node.children[0]); a.style.flex = `${node.sizes[0]} 1 0`;
    const sp = document.createElement('div'); sp.className = `splitter ${node.dir}`;
    const b = this._renderNode(node.children[1]); b.style.flex = `${node.sizes[1]} 1 0`;
    this._wireSplitter(sp, node, a, b);
    el.append(a, sp, b);
    return el;
  }
  _buildPane(leafId) {
    const c = this.content.get(leafId);
    const pane = document.createElement('div');
    pane.className = 'pane' + (leafId === this.active.focusedLeafId ? ' is-focused' : '');
    const head = document.createElement('div'); head.className = 'pane-head';
    const title = document.createElement('div'); title.className = 'pane-title'; title.textContent = c?.title || 'pane';
    const tools = document.createElement('div'); tools.className = 'pane-tools';
    tools.innerHTML =
      `<button data-act="split-right" title="split right">${ICON.splitRight}</button>` +
      `<button data-act="split-down" title="split down">${ICON.splitDown}</button>` +
      `<button data-act="detach" title="break out to window">${ICON.detach}</button>` +
      `<button data-act="close" class="close" title="close pane">${ICON.close}</button>`;
    tools.addEventListener('click', (e) => {
      const b = e.target.closest('button'); if (!b) return;
      const act = b.dataset.act;
      if (act === 'split-right') this.splitLeaf(leafId, 'row');
      else if (act === 'split-down') this.splitLeaf(leafId, 'col');
      else if (act === 'detach') this.detachLeaf(leafId);
      else if (act === 'close') this.closeLeaf(leafId);
    });
    head.append(title, tools);
    this._wireHeadDetach(head, leafId);
    const body = document.createElement('div'); body.className = 'pane-body';
    if (c) body.appendChild(c.el);              // move live content in (preserves state)
    pane.append(head, body);
    pane.addEventListener('mousedown', () => { this.active.focusedLeafId = leafId; this._refocus(); });
    return pane;
  }
  _refocus() {
    this.panes.querySelectorAll('.pane').forEach(p => p.classList.remove('is-focused'));
    // cheap: re-mark without full re-render
    const c = this.content.get(this.active.focusedLeafId);
    if (c) { const el = c.el.closest('.pane'); el && el.classList.add('is-focused'); c.focus && c.focus(); }
  }

  /* drag the pane header OFF the tiling area → break out into its own window */
  _wireHeadDetach(head, leafId) {
    head.addEventListener('pointerdown', (e) => {
      if (e.target.closest('.pane-tools')) return;        // tool clicks handled elsewhere
      const startX = e.clientX, startY = e.clientY;
      let armed = false;
      const move = (ev) => {
        if (!armed && Math.hypot(ev.clientX - startX, ev.clientY - startY) > 6) armed = true;
      };
      const up = (ev) => {
        document.removeEventListener('pointermove', move);
        document.removeEventListener('pointerup', up);
        if (!armed) return;
        const r = this.panes.getBoundingClientRect();
        const outside = ev.clientX < r.left || ev.clientX > r.right || ev.clientY < r.top || ev.clientY > r.bottom;
        if (outside) this.detachLeaf(leafId);
      };
      document.addEventListener('pointermove', move);
      document.addEventListener('pointerup', up);
    });
  }

  _wireSplitter(sp, node, elA, elB) {
    const dir = node.dir;
    sp.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      sp.setPointerCapture(e.pointerId);
      sp.classList.add('dragging');
      const rect = sp.parentElement.getBoundingClientRect();
      const total = dir === 'row' ? rect.width : rect.height;
      const start = dir === 'row' ? e.clientX : e.clientY;
      const sum = node.sizes[0] + node.sizes[1];
      const s0 = node.sizes[0];
      const move = (ev) => {
        const cur = dir === 'row' ? ev.clientX : ev.clientY;
        const df = ((cur - start) / total) * sum;
        const n0 = Math.max(0.08 * sum, Math.min(0.92 * sum, s0 + df));
        const n1 = sum - n0;
        node.sizes = [n0, n1];
        elA.style.flex = `${n0} 1 0`; elB.style.flex = `${n1} 1 0`;
      };
      const up = () => {
        sp.classList.remove('dragging');
        document.removeEventListener('pointermove', move);
        document.removeEventListener('pointerup', up);
      };
      document.addEventListener('pointermove', move);
      document.addEventListener('pointerup', up);
    });
  }
}

/* ---------- tree utilities ---------- */
function locate(root, pred, parent = null, index = -1) {
  if (pred(root)) return { node: root, parent, index };
  if (root.type === 'split') {
    for (let i = 0; i < root.children.length; i++) {
      const r = locate(root.children[i], pred, root, i);
      if (r) return r;
    }
  }
  return null;
}
function firstLeaf(node) { return node.type === 'leaf' ? node.leafId : firstLeaf(node.children[0]); }
function eachLeaf(node, fn) { if (node.type === 'leaf') fn(node.leafId); else node.children.forEach(c => eachLeaf(c, fn)); }
