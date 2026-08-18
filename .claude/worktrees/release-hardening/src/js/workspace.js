/* ============================================================
   workspace.js — Tilix-shaped chrome.
   • tabs (add / close / drag-reorder)
   • binary tiling tree (split-right / split-down)
   • draggable splitters (live resize)
   • per-pane triple-button + close
   • UNIFIED pane drag (grab a pane header), three drop targets:
       1. release OUTSIDE the tiling area  → break the pane out into its own window
       2. onto a TAB in the top bar         → move the pane into that tab
       3. onto another PANE (sub-tab)       → edge-aware split:
            right → split row,  dragged lands right    left  → lands left
            bottom→ split col,  dragged lands bottom    top   → lands top

   The node tree only references leafIds; live content lives in contentMap so
   panes keep their state (terminal scrollback, input) across splits & moves —
   moving a leaf between tabs is just a tree edit, the content map is global.
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
    // --- pane drag state ---
    this._dragLeafId = null;       // leaf currently being dragged
    this._dropHandled = false;     // a valid drop consumed the drag (suppress breakout)
    this._dropEl = null;           // reusable drop-zone indicator overlay
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

  /* Detach a leaf node from whatever tab/tree holds it WITHOUT destroying its
     content, promoting its sibling in place. Returns the orphaned leaf node
     (or null). Empties-then-removes the source tab if the leaf was its root. */
  _extractLeaf(leafId) {
    for (const tab of this.tabs) {
      const loc = locate(tab.root, n => n.type === 'leaf' && n.leafId === leafId);
      if (!loc) continue;
      const node = loc.node;
      if (!loc.parent) {
        // leaf was the entire tab — drop the tab shell (content kept for the move)
        const i = this.tabs.indexOf(tab);
        if (i >= 0) this.tabs.splice(i, 1);
        if (this.active === tab) this.active = this.tabs[Math.max(0, i - 1)] || null;
      } else {
        const sibling = loc.parent.children[1 - loc.index];
        const gp = locate(tab.root, n => n === loc.parent);
        if (!gp.parent) tab.root = sibling; else gp.parent.children[gp.index] = sibling;
        if (tab.focusedLeafId === leafId) tab.focusedLeafId = firstLeaf(tab.root);
      }
      return node;
    }
    return null;
  }

  /* Target #3 — drop a pane onto another pane, splitting it along the edge. */
  dropPaneOnLeaf(srcLeafId, targetLeafId, edge) {
    if (!srcLeafId || srcLeafId === targetLeafId) return;
    const src = this._extractLeaf(srcLeafId);
    if (!src) return;
    // locate target AFTER extraction (the tree may have collapsed a level)
    let targetTab = null, loc = null;
    for (const tab of this.tabs) {
      const l = locate(tab.root, n => n.type === 'leaf' && n.leafId === targetLeafId);
      if (l) { targetTab = tab; loc = l; break; }
    }
    if (!targetTab) { this.addTab(src, this.content.get(src.leafId)?.title, true); return; }
    const dir = (edge === 'left' || edge === 'right') ? 'row' : 'col';
    const children = (edge === 'right' || edge === 'bottom') ? [loc.node, src] : [src, loc.node];
    const split = { type: 'split', dir, children, sizes: [0.5, 0.5] };
    if (!loc.parent) targetTab.root = split; else loc.parent.children[loc.index] = split;
    targetTab.focusedLeafId = src.leafId;
    this.active = targetTab;
    this.renderTabs(); this.renderPanes();
  }

  /* Target #2 — drop a pane onto a tab: merge it into that tab's tree. */
  dropPaneOnTab(srcLeafId, tabId) {
    const target = this.tabs.find(t => t.id === tabId);
    if (!target || !srcLeafId) return;
    // no-op if the pane is already the entire content of the target tab
    if (target.root.type === 'leaf' && target.root.leafId === srcLeafId) return;
    const src = this._extractLeaf(srcLeafId);
    if (!src) return;
    const tt = this.tabs.find(t => t.id === tabId);     // may have been removed if src was its root
    if (!tt) { this.addTab(src, this.content.get(src.leafId)?.title, true); return; }
    tt.root = { type: 'split', dir: 'row', sizes: [0.6, 0.4], children: [tt.root, src] };
    tt.focusedLeafId = src.leafId;
    this.active = tt;
    this.renderTabs(); this.renderPanes();
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
      // drag source: reorder
      el.addEventListener('dragstart', (e) => { e.dataTransfer.setData('text/tab', String(idx)); e.dataTransfer.effectAllowed = 'move'; });
      // drop target: accept a tab (reorder) OR a pane (merge into this tab)
      el.addEventListener('dragover', (e) => {
        const t = e.dataTransfer.types;
        if (t.includes('text/pane')) { e.preventDefault(); el.classList.add('pane-drop-over'); }
        else if (t.includes('text/tab')) { e.preventDefault(); el.classList.add('drag-over'); }
      });
      el.addEventListener('dragleave', () => el.classList.remove('drag-over', 'pane-drop-over'));
      el.addEventListener('drop', (e) => {
        e.preventDefault(); el.classList.remove('drag-over', 'pane-drop-over');
        if (e.dataTransfer.types.includes('text/pane')) {
          this._dropHandled = true;
          this.dropPaneOnTab(this._dragLeafId, tab.id);
        } else if (e.dataTransfer.types.includes('text/tab')) {
          this.moveTab(Number(e.dataTransfer.getData('text/tab')), idx);
        }
      });
      this.strip.appendChild(el);
    });
    const add = document.createElement('button');
    add.className = 'tab-add'; add.title = 'new tab'; add.textContent = '+';
    add.addEventListener('click', () => this.addTab(null, `shell ${this.tabSeq + 1}`));
    // drop a pane on "+" → break it into a brand-new tab
    add.addEventListener('dragover', (e) => { if (e.dataTransfer.types.includes('text/pane')) { e.preventDefault(); add.classList.add('pane-drop-over'); } });
    add.addEventListener('dragleave', () => add.classList.remove('pane-drop-over'));
    add.addEventListener('drop', (e) => {
      if (!e.dataTransfer.types.includes('text/pane')) return;
      e.preventDefault(); add.classList.remove('pane-drop-over');
      this._dropHandled = true;
      const src = this._extractLeaf(this._dragLeafId);
      if (src) this.addTab(src, this.content.get(src.leafId)?.title, true);
    });
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
    this._dropEl = null;                       // overlay is rebuilt lazily per drag
    if (!this.active) return;
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
    // the header is the drag handle for the whole pane
    head.draggable = true;
    this._wirePaneDrag(head, pane, leafId);
    this._wirePaneDrop(pane, leafId);
    const body = document.createElement('div'); body.className = 'pane-body';
    if (c) body.appendChild(c.el);              // move live content in (preserves state)
    pane.append(head, body);
    pane.addEventListener('mousedown', () => { this.active.focusedLeafId = leafId; this._refocus(); });
    return pane;
  }
  _refocus() {
    this.panes.querySelectorAll('.pane').forEach(p => p.classList.remove('is-focused'));
    const c = this.content.get(this.active.focusedLeafId);
    if (c) { const el = c.el.closest('.pane'); el && el.classList.add('is-focused'); c.focus && c.focus(); }
  }

  /* ---------- pane drag & drop (the unified gesture) ---------- */
  _wirePaneDrag(head, pane, leafId) {
    head.addEventListener('dragstart', (e) => {
      if (e.target.closest('.pane-tools')) { e.preventDefault(); return; }  // let tool buttons click
      this._dragLeafId = leafId;
      this._dropHandled = false;
      const c = this.content.get(leafId);
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/pane', leafId);
      // descriptor for a future cross-window handshake (BroadcastChannel)
      e.dataTransfer.setData('text/td-pane', JSON.stringify({ paneType: c?.paneType, title: c?.title }));
      pane.classList.add('is-dragging');
    });
    head.addEventListener('dragend', (e) => {
      pane.classList.remove('is-dragging');
      this._clearDropZone();
      // Target #1 — released on nothing, outside the tiling area → break out to a window
      if (!this._dropHandled) {
        const r = this.panes.getBoundingClientRect();
        const outside = e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
        if (outside) this.detachLeaf(leafId);
      }
      this._dragLeafId = null;
      this._dropHandled = false;
    });
  }
  _wirePaneDrop(pane, leafId) {
    pane.addEventListener('dragover', (e) => {
      if (!this._dragLeafId || !e.dataTransfer.types.includes('text/pane')) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      if (leafId === this._dragLeafId) { this._clearDropZone(); return; }   // can't drop onto self
      const edge = edgeFromEvent(pane.getBoundingClientRect(), e.clientX, e.clientY);
      this._showDropZone(pane, edge);
    });
    pane.addEventListener('dragleave', (e) => {
      // only clear when actually leaving the pane (not crossing into a child)
      if (!pane.contains(e.relatedTarget)) this._clearDropZone();
    });
    pane.addEventListener('drop', (e) => {
      if (!e.dataTransfer.types.includes('text/pane')) return;
      e.preventDefault();
      const edge = edgeFromEvent(pane.getBoundingClientRect(), e.clientX, e.clientY);
      this._clearDropZone();
      this._dropHandled = true;
      this.dropPaneOnLeaf(this._dragLeafId, leafId, edge);
    });
  }
  _showDropZone(pane, edge) {
    if (!this._dropEl) {
      this._dropEl = document.createElement('div');
      this._dropEl.className = 'drop-indicator';
    }
    if (this._dropEl.parentElement !== pane) pane.appendChild(this._dropEl);
    const half = { left:   { left:0, top:0, right:'50%', bottom:0 },
                   right:  { left:'50%', top:0, right:0, bottom:0 },
                   top:    { left:0, top:0, right:0, bottom:'50%' },
                   bottom: { left:0, top:'50%', right:0, bottom:0 } }[edge];
    Object.assign(this._dropEl.style, { left:'', top:'', right:'', bottom:'' }, half);
  }
  _clearDropZone() {
    if (this._dropEl && this._dropEl.parentElement) this._dropEl.parentElement.removeChild(this._dropEl);
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

/* Which edge of `rect` is the cursor nearest? → 'left' | 'right' | 'top' | 'bottom' */
function edgeFromEvent(rect, x, y) {
  const lx = (x - rect.left) / Math.max(1, rect.width);
  const ly = (y - rect.top) / Math.max(1, rect.height);
  const d = { left: lx, right: 1 - lx, top: ly, bottom: 1 - ly };
  return Object.keys(d).reduce((a, b) => (d[b] < d[a] ? b : a));
}
