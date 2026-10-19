/* decision-brief — reader notes.
 *
 * Drop-in. No build step, no dependencies. Paste this file's contents inline inside a
 * script element, after the document body — a decision brief must survive being emailed
 * as one file, so never load it with a src attribute.
 *
 * NOTHING IN THIS FILE MAY CONTAIN THE LITERAL CLOSING SCRIPT TAG, not even inside a
 * comment or a string: an HTML parser ends the script element at the first one it sees,
 * and everything after it silently becomes page content. Write it broken up, e.g.
 * "<\/script>", if you ever need to emit one.
 *
 * Three outputs, in the order they matter:
 *   1. copy map    — anchored note map, ~1% of the tokens of the whole document
 *   2. save into   — bakes notes into a downloadable copy so an agent can READ them
 *   3. localStorage — live persistence between reloads
 *
 * Why the map exists: pasting a whole annotated report into a prompt costs thousands of
 * tokens; pasting bare notes loses which passage each one is about. The map is the third
 * option — [element-id] + note, so the agent greps the id and finds the passage itself.
 *
 * CONCUR. Every decision (.ask) also gets a space on its right that takes a rubber stamp —
 * the commonest answer a decision gets, and the quickest way to say the work was good.
 * A concur is independent of notes, rides in the map as "✓ concur" on the decision's own
 * line, and is baked into its own island (#report-concurs) so the notes island keeps the
 * shape every saved brief already has.
 *
 * FORMAT 1. Other programs write these islands too (Terminal Delight edits a brief's notes
 * in place), so what a save writes is a contract, pinned byte for byte by the fixtures in
 * fixtures/notes-format/ and its reference writer. Each island carries data-format and a
 * revision, data-rev, the time of the write. Island JSON escapes "<" before "/" and "!",
 * and the plain-text mirror breaks any sequence that would end an HTML comment. A change
 * to any of that bumps FORMAT and the fixtures in the same commit.
 *
 * Configure before this script runs (all optional):
 *   window.NOTES_FILE     — filename used for the storage key and the download. Default: location basename.
 *   window.NOTES_TARGETS  — [[selector, prefix], ...] overriding what gets an affordance.
 *   window.CONCUR_TARGETS — selector for what takes a concur stamp. Default: the grill's decisions.
 */
(function () {
  'use strict';

  var FILE = window.NOTES_FILE ||
    (location.pathname.split('/').pop() || 'decision-brief.html');
  var KEY = 'notes:' + FILE;

  /* Default targets. A prefix keeps exported anchors readable and grouped.
     Override wholesale with window.NOTES_TARGETS if a brief's shape differs.

     THE GRILL COMES FIRST, AND IT IS NOT OPTIONAL. Until 2026-09-02 this list
     covered five of the seven components layout.md tells you to build and
     omitted the grill — so the one section whose entire purpose is a decision
     the reader has to make was the one section the reader could not comment on.
     Two briefs shipped that way, each having invented its own class name
     (`grill-q`, then `q`) because nothing here named one. Both spellings are
     matched below; `.ask` is the name to use going forward. If you add a
     component, add it here in the same commit.

     FIGURES ARE SECOND, for the same reason. The 2026-09-15 attention-spine
     brief carried eleven pictures and had to override NOTES_TARGETS wholesale
     to make any of them annotatable, because `figure` was not here — a fork of
     this list living in one report, where nothing else would inherit it. The
     pictures are the part a reader argues with, so they are in the default list
     now. `titleOf()` already reads `.lbl`, so a figure labelled `03 · What
     opening costs` anchors as `fig-03-what-opening-costs`. */
  var TARGETS = window.NOTES_TARGETS || [
    ['.grill .q, .grill-q, .ask', 'ask'],
    ['figure', 'fig'],
    ['.card', 'verdict'],
    ['.esc-list li', 'readfirst'],
    ['.finding', 'finding'],
    ['.tile', 'stat'],
    ['table.wide tr', 'row'],
    ['.bar-row', 'chart'],
    ['.callout', 'callout'],
    ['dialog .dlg-body > h4', 'sub']
  ];

  var FORMAT = '1';
  /* Concurs: { anchor: 'YYYY-MM-DD HH:MM' }, under their own key and island. */
  var CKEY = 'concurs:' + FILE;
  var SKEY = 'notes-sync:' + FILE;
  var CONCUR_SEL = window.CONCUR_TARGETS || '.grill .q, .grill-q, .ask';

  /* ---- which notes the page shows: the file's and this browser's, together ----
     Notes live in two places: the islands baked into this file, which is what an agent
     reads, and localStorage, which keeps a reader's edits between reloads. Before format 1
     the stored copy simply won, so once a browser had stored any map for this filename,
     even the {} the clear button leaves, notes written into the file by anyone else never
     showed again (agent-skills#24).

     So every save stamps a revision on the islands, and this browser keeps, under SKEY,
     the revision it last took in and every note and concur it has taken from an island or
     written into one. On load, for notes and concurs alike:
       - nothing stored: the island, as before;
       - the island's revision is the one last taken in: the stored copy, as before;
       - anything else, including an island with no revision (a brief saved before
         format 1): the stored copy, plus every note and concur in the island this browser
         has not seen before.
     Nothing the browser holds is ever dropped, and a note it deleted stays deleted. The
     price: a note deleted from the file by another writer stays in a browser that still
     holds it. Revisions are compared for sameness, not order, because a save lands in
     Downloads while the original may have been changed in the meantime. */
  function stored(key) {
    try {
      var v = JSON.parse(localStorage.getItem(key) || 'null');
      return v && typeof v === 'object' && !Array.isArray(v) ? v : null;
    } catch (e) { return null; }
  }
  function readIsland(id) {
    var el = document.getElementById(id), map = {};
    if (el) {
      try { map = JSON.parse(el.textContent || '{}'); } catch (e) { /* a malformed island must not break the page */ }
      if (!map || typeof map !== 'object' || Array.isArray(map)) map = {};
    }
    return { map: map, rev: el ? el.getAttribute('data-rev') : null };
  }
  function has(o, k) { return Object.prototype.hasOwnProperty.call(o, k); }
  function noteId(nid, n) { return nid + '\n' + (n.ts || '') + '\n' + n.text; }
  function idsOf(kind, map) {
    var out = [];
    for (var k in map) {
      if (!has(map, k)) continue;
      if (kind === 'concurs') out.push(k + '\n' + map[k]);
      else if (Array.isArray(map[k])) map[k].forEach(function (n) { if (n) out.push(noteId(k, n)); });
    }
    return out;
  }
  function union(a, b) {
    var seen = Object.create(null), out = [];
    a.concat(b).forEach(function (x) { if (!seen[x]) { seen[x] = 1; out.push(x); } });
    return out;
  }

  var SYNC = stored(SKEY) || {};
  ['notes', 'concurs'].forEach(function (k) {
    var s = SYNC[k];
    if (!s || typeof s !== 'object' || !Array.isArray(s.seen)) SYNC[k] = { rev: null, seen: [] };
  });

  function takeIn(kind, key, isl) {
    var s = SYNC[kind], have = stored(key), added = false;
    if (have && !(isl.rev && isl.rev === s.rev)) {
      var seen = Object.create(null), mine = Object.create(null);
      s.seen.forEach(function (x) { seen[x] = 1; });
      idsOf(kind, have).forEach(function (x) { mine[x] = 1; });
      Object.keys(isl.map).forEach(function (k) {
        if (kind === 'concurs') {
          if (!has(have, k) && !seen[k + '\n' + isl.map[k]]) { have[k] = isl.map[k]; added = true; }
          return;
        }
        if (!Array.isArray(isl.map[k])) return;
        isl.map[k].forEach(function (n) {
          var id = n && noteId(k, n);
          if (!n || seen[id] || mine[id]) return;
          if (!Array.isArray(have[k])) have[k] = [];
          have[k].push(n);
          mine[id] = 1;
          added = true;
        });
      });
    }
    SYNC[kind] = { rev: isl.rev, seen: union(s.seen, idsOf(kind, isl.map)) };
    return { map: have || isl.map, added: added };
  }

  var tookN = takeIn('notes', KEY, readIsland('report-notes'));
  var tookC = takeIn('concurs', CKEY, readIsland('report-concurs'));
  var N = tookN.map, C = tookC.map;
  if (tookN.added) save();
  if (tookC.added) saveConcurs();

  var reduceMotion = !!(window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches);
  var active = null, activeTitle = '';

  /* The sync record goes with every write, so what it says was seen is always stored. */
  function save() {
    try { localStorage.setItem(KEY, JSON.stringify(N)); localStorage.setItem(SKEY, JSON.stringify(SYNC)); } catch (e) {}
  }
  function saveConcurs() {
    try { localStorage.setItem(CKEY, JSON.stringify(C)); localStorage.setItem(SKEY, JSON.stringify(SYNC)); } catch (e) {}
  }
  function stamp() { return new Date().toISOString().slice(0, 16).replace('T', ' '); }
  function concurCount() {
    var n = 0;
    for (var k in C) { if (C.hasOwnProperty(k)) n++; }
    return n;
  }
  function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }
  function slug(s) {
    return String(s || 'x').toLowerCase().replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '').slice(0, 44) || 'x';
  }
  function titleOf(el) {
    var h = el.querySelector('h2,h3,h4,.bar-lab,.k,.lbl,td:first-child,th:first-child');
    var t = ((h ? h.textContent : el.textContent) || 'item').replace(/\s+/g, ' ').trim();
    return t.length > 72 ? t.slice(0, 69) + '…' : t;
  }

  /* ---- give every target a stable anchor id and a note affordance ---- */
  function tag() {
    var used = {};
    TARGETS.forEach(function (pair) {
      var sel = pair[0], pre = pair[1];
      Array.prototype.forEach.call(document.querySelectorAll(sel), function (el, i) {
        if (el.classList.contains('notable')) return;
        if (el.tagName === 'TR' && el.querySelector('th')) return;   // header rows
        if (el.closest('#d-note, #d-export, .notebar')) return;

        /* Numbered lists anchor by position; everything else by a SHORT title slug.
           Long slugs make the exported map unreadable, which defeats the map. */
        var id;
        var num = el.querySelector('.esc-n');
        if (num) id = pre + '-' + num.textContent.trim();
        else id = pre + '-' + slug(titleOf(el)).split('-').slice(0, 4).join('-');

        var base = id, n = 2;
        while (used[id]) id = base + '-' + (n++);
        used[id] = 1;

        el.classList.add('notable');
        el.dataset.nid = id;
        el.dataset.ntitle = titleOf(el);
        if (!el.id) el.id = id;                      // a real anchor in the file

        var b = document.createElement('button');
        b.className = 'note-btn';
        b.type = 'button';
        b.title = 'Add a note';
        b.innerHTML = '&#x1F4AC;';
        b.addEventListener('click', function (ev) {
          ev.preventDefault(); ev.stopPropagation(); open(el);
        });
        el.appendChild(b);
      });
    });
  }

  /* ---- concur: a rubber stamp in a space on each decision's right ----
     The angle is seeded from the anchor, so a stamp lands the same way on every reload
     and a page of them still looks hand-done. */
  function seed(s) {
    var h = 2166136261;
    for (var i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
    return (h >>> 0) / 4294967295;
  }
  function stampSvg(id) {
    var r = -12 + 9 * seed(id), dx = (seed(id + ':x') - 0.5) * 6, dy = (seed(id + ':y') - 0.5) * 6;
    var ink = '#35c27a';
    return '<svg class="concur-stamp" viewBox="0 0 120 120" aria-label="concur stamp" style="--r:' +
      r.toFixed(1) + 'deg;--dx:' + dx.toFixed(1) + 'px;--dy:' + dy.toFixed(1) + 'px">' +
      '<g filter="url(#concur-ink)">' +
      '<rect x="9" y="33" width="102" height="54" rx="7" fill="none" stroke="' + ink + '" stroke-width="5"/>' +
      '<rect x="16" y="40" width="88" height="40" rx="3" fill="none" stroke="' + ink + '" stroke-width="1.6"/></g>' +
      '<g filter="url(#concur-ink-fine)">' +
      '<path d="M22 61 L28 67 L39 52" fill="none" stroke="' + ink + '" stroke-width="3.6" stroke-linecap="round" stroke-linejoin="round"/>' +
      '<text x="70" y="69" text-anchor="middle" textLength="58" lengthAdjust="spacingAndGlyphs" fill="' + ink + '" ' +
      'font-family="Anton, Oswald, Impact, \'Arial Black\', \'DejaVu Sans Condensed\', sans-serif" ' +
      'font-weight="800" font-size="21">CONCUR</text></g></svg>';
  }

  /* Wet ink. The strokes swell a little, a fine speckle and a few round air pockets are cut
     out of them, then a blur and a hard alpha curve round every edge and every hole the way
     ink sits on paper, and a slow wobble bends the whole. The frame takes big pockets; the
     check and the word take small ones, because pockets that size eat a condensed letter.
     The pockets are sized to survive a snapshot: Terminal Delight's document square draws a
     screenshot of the page, where the old two-pixel speckle smoothed away. One copy per page. */
  function wetInk(id, pocketFreq, pocketAt, speckCut, blur, swell) {
    return '<filter id="' + id + '" x="-15%" y="-15%" width="130%" height="130%">' +
      '<feMorphology in="SourceGraphic" operator="dilate" radius="' + swell + '" result="fat"/>' +
      '<feTurbulence type="fractalNoise" baseFrequency="0.6" numOctaves="2" seed="11" result="n"/>' +
      '<feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  -9 0 0 0 ' + speckCut + '" result="speck"/>' +
      '<feTurbulence type="fractalNoise" baseFrequency="' + pocketFreq + '" numOctaves="3" seed="3" result="p"/>' +
      '<feColorMatrix in="p" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  -22 0 0 0 ' + (22 * pocketAt).toFixed(2) + '" result="pock"/>' +
      '<feComposite in="speck" in2="pock" operator="in" result="mask"/>' +
      '<feComposite in="fat" in2="mask" operator="in" result="inked"/>' +
      '<feGaussianBlur in="inked" stdDeviation="' + blur + '" result="soft"/>' +
      '<feComponentTransfer in="soft" result="wet"><feFuncA type="linear" slope="5" intercept="-1.6"/></feComponentTransfer>' +
      '<feTurbulence type="fractalNoise" baseFrequency="0.035" numOctaves="2" seed="5" result="w"/>' +
      '<feDisplacementMap in="wet" in2="w" scale="3" xChannelSelector="R" yChannelSelector="G"/>' +
      '</filter>';
  }
  function concurDefs() {
    if (document.getElementById('concur-defs')) return;
    var d = document.createElement('div');
    d.innerHTML = '<svg id="concur-defs" viewBox="0 0 1 1" aria-label="concur stamp ink" focusable="false" ' +
      'style="position:absolute;width:0;height:0;overflow:hidden"><defs>' +
      wetInk('concur-ink', 0.17, 0.62, 6.0, 0.55, 0.4) +
      wetInk('concur-ink-fine', 0.35, 0.72, 7.0, 0.3, 0.28) +
      '</defs></svg>';
    document.body.appendChild(d.firstChild);
  }

  function showConcur(el, zone, on, animate) {
    var live = zone.querySelector('.concur-stamp:not(.peel)');
    if (on && !live) {
      zone.insertAdjacentHTML('beforeend', stampSvg(el.dataset.nid));
      if (animate && !reduceMotion) {
        zone.querySelector('.concur-stamp:not(.peel)').classList.add('thud');
        setTimeout(function () {
          el.classList.remove('concur-jolt'); void el.offsetWidth; el.classList.add('concur-jolt');
        }, 110);
      }
    } else if (!on && live) {
      if (animate && !reduceMotion) {
        live.classList.add('peel');
        setTimeout(function () { live.remove(); }, 320);
      } else live.remove();
    }
    zone.classList.toggle('on', on);
    el.classList.toggle('has-concur', on);
    zone.setAttribute('aria-pressed', on ? 'true' : 'false');
    zone.title = on ? 'Concurred. Click to peel it off' : 'Concur with this decision';
  }

  function concurZones() {
    var made = 0;
    Array.prototype.forEach.call(document.querySelectorAll(CONCUR_SEL), function (el) {
      if (!el.dataset.nid || el.querySelector(':scope > .concur-zone')) return;
      if (el.closest('#d-note, #d-export, .notebar')) return;
      var zone = document.createElement('button');
      zone.type = 'button';
      zone.className = 'concur-zone';
      zone.innerHTML = '<span class="concur-q" aria-hidden="true">?</span><span class="concur-lbl">concur</span>';
      zone.addEventListener('click', function (ev) {
        ev.preventDefault(); ev.stopPropagation();
        var id = el.dataset.nid;
        if (C[id]) delete C[id]; else C[id] = stamp();
        saveConcurs();
        showConcur(el, zone, !!C[id], true);
        badges();
      });
      el.classList.add('concurrable');
      el.appendChild(zone);
      showConcur(el, zone, !!C[el.dataset.nid], false);
      made++;
    });
    if (!made) return;
    concurDefs();
    var bar = document.getElementById('notebar');
    if (bar && !document.getElementById('concur-cnt')) {
      var s = document.createElement('span');
      s.className = 'cnt';
      s.id = 'concur-cnt';
      s.innerHTML = '&middot; <b id="concur-count">0</b> <span id="concur-word">concurs</span>';
      var first = bar.querySelector('.cnt');
      if (first) first.parentNode.insertBefore(s, first.nextSibling); else bar.insertBefore(s, bar.firstChild);
    }
  }

  function badges() {
    var total = 0;
    Array.prototype.forEach.call(document.querySelectorAll('.notable'), function (el) {
      var n = (N[el.dataset.nid] || []).length;
      total += n;
      el.classList.toggle('has-note', n > 0);
      var b = el.querySelector(':scope > .note-btn');
      if (b) b.innerHTML = n ? String(n) : '&#x1F4AC;';
    });
    var c = document.getElementById('note-count');
    if (c) c.textContent = total;
    var cc = concurCount();
    var ce = document.getElementById('concur-count');
    if (ce) ce.textContent = cc;
    var cw = document.getElementById('concur-word');
    if (cw) cw.textContent = cc === 1 ? 'concur' : 'concurs';
    /* A concur alone is an answer worth exporting; clear stays notes-only. */
    ['btn-export', 'btn-embed'].forEach(function (id) {
      var b = document.getElementById(id);
      if (b) b.disabled = total + cc === 0;
    });
    var bc = document.getElementById('btn-clear');
    if (bc) bc.disabled = total === 0;
  }

  function render() {
    var list = N[active] || [];
    var ul = document.getElementById('note-list');
    document.getElementById('note-none').style.display = list.length ? 'none' : 'block';
    ul.innerHTML = list.map(function (n, i) {
      return '<li><div class="nts"><span>' + esc(n.ts) + '</span>' +
        '<button class="ndel" data-i="' + i + '">delete</button></div>' + esc(n.text) + '</li>';
    }).join('');
    Array.prototype.forEach.call(ul.querySelectorAll('.ndel'), function (b) {
      b.addEventListener('click', function () { del(+b.dataset.i); });
    });
  }

  function open(el) {
    active = el.dataset.nid;
    activeTitle = el.dataset.ntitle;
    document.getElementById('note-title').textContent = activeTitle;
    document.getElementById('note-anchor').textContent = '#' + active;
    render();
    document.getElementById('d-note').showModal();
    document.getElementById('note-text').focus();
  }

  function add() {
    var ta = document.getElementById('note-text');
    var t = ta.value.trim();
    if (!t || !active) return;
    (N[active] = N[active] || []).push({
      text: t,
      title: activeTitle,
      ts: new Date().toISOString().slice(0, 16).replace('T', ' ')
    });
    ta.value = '';
    save(); render(); badges();
  }

  function del(i) {
    N[active].splice(i, 1);
    if (!N[active].length) delete N[active];
    save(); render(); badges();
  }

  /* ---- the map: anchored, compact, prompt-ready ---- */
  function buildMap() {
    var count = 0, els = 0;
    for (var k in N) { if (N.hasOwnProperty(k)) { els++; count += N[k].length; } }
    var cc = concurCount();
    var out = [
      'NOTES — ' + FILE,
      count + ' notes on ' + els + ' elements' +
        (cc ? ' · ' + cc + (cc === 1 ? ' concur' : ' concurs') : '') + '.',
      'Each [anchor] is an element id in that file — search it to find the passage.',
      ''
    ];
    /* Document order, not object order: the map should read the way the brief reads.
       A concur rides on the decision's own line, so an agreed decision with no note
       still reaches the agent. */
    Array.prototype.forEach.call(document.querySelectorAll('.notable'), function (el) {
      var list = N[el.dataset.nid] || [];
      var agreed = !!C[el.dataset.nid];
      if (!list.length && !agreed) return;
      out.push('[' + el.dataset.nid + '] ' + el.dataset.ntitle + (agreed ? '  ✓ concur' : ''));
      list.forEach(function (n) { out.push('  · ' + n.text.replace(/\n+/g, ' ')); });
      out.push('');
    });
    return out.join('\n');
  }

  function exportNotes() {
    var txt = buildMap();
    document.getElementById('export-out').value = txt;
    var est = document.getElementById('export-est');
    if (est) {
      est.textContent = txt.length + ' characters ≈ ' + Math.ceil(txt.length / 4) +
        ' tokens — versus ~' + Math.ceil(document.body.innerText.length / 4) +
        ' for the whole document.';
    }
    document.getElementById('d-export').showModal();
  }

  function copyExport(btn) {
    var ta = document.getElementById('export-out');
    ta.select();
    try { document.execCommand('copy'); } catch (e) {}
    if (btn) {
      var o = btn.textContent;
      btn.textContent = 'Copied';
      setTimeout(function () { btn.textContent = o; }, 1400);
    }
  }

  /* ---- bake notes into a downloadable copy of this file ----
     localStorage is invisible to an agent. This is what makes
     "read my notes in <file>" work. */

  /* JSON leaves "<" alone, so a note reading "<\/script>" used to end the island early:
     the rest spilled into the page and the reopened brief showed no notes at all
     (agent-skills#25). Escaped, it is still JSON and parses to the same strings. */
  function islandJson(map) {
    return JSON.stringify(map, null, 1).replace(/<\//g, '<\\/').replace(/<!/g, '\\u003c!');
  }
  /* The only two sequences that end an HTML comment. A reader's own "---" stays. */
  function commentSafe(s) {
    return s.replace(/--!>/g, '--! >').replace(/-->/g, '-- >');
  }
  function stampIsland(el, rev) {
    el.setAttribute('data-format', FORMAT);
    el.setAttribute('data-rev', rev);
  }
  /* The mirror's first line, written once here so a grep of a brief for it finds one
     line of code and then the mirror itself. */
  var MIRROR_HEAD = '\nREADER NOTES —';
  /* The last mirror comment, if it follows the notes island: the one a save replaces. */
  function lastMirror(root, island) {
    var w = document.createTreeWalker(root, NodeFilter.SHOW_COMMENT), c, last = null;
    while ((c = w.nextNode())) if (c.data.indexOf(MIRROR_HEAD) === 0) last = c;
    if (last && island && !(island.compareDocumentPosition(last) & Node.DOCUMENT_POSITION_FOLLOWING)) return null;
    return last;
  }

  function embedNotes() {
    var rev = new Date().toISOString();
    var doc = document.documentElement.cloneNode(true);
    Array.prototype.forEach.call(doc.querySelectorAll('.note-btn'), function (n) { n.remove(); });
    Array.prototype.forEach.call(doc.querySelectorAll('.notable'), function (n) {
      n.classList.remove('notable', 'has-note');
    });
    var island = doc.querySelector('#report-notes');
    if (island) { island.textContent = islandJson(N); stampIsland(island, rev); }

    /* Concur UI is rebuilt on load, so strip it; the answers go into their own island,
       created here, where the reference writer puts it, if the brief predates it and
       there is something to put in it. */
    Array.prototype.forEach.call(doc.querySelectorAll('.concur-zone, #concur-defs, #concur-cnt'),
      function (n) { n.remove(); });
    Array.prototype.forEach.call(doc.querySelectorAll('.concurrable'), function (n) {
      n.classList.remove('concurrable', 'has-concur', 'concur-jolt');
    });
    var cisl = doc.querySelector('#report-concurs');
    if (!cisl && concurCount()) {
      cisl = document.createElement('script');
      cisl.type = 'application/json';
      cisl.id = 'report-concurs';
      if (island && island.parentNode) island.parentNode.insertBefore(cisl, island.nextSibling);
      else if (doc.querySelector('body')) doc.querySelector('body').appendChild(cisl);
    }
    if (cisl) { cisl.textContent = islandJson(C); stampIsland(cisl, rev); }

    /* A plain-text mirror as well, so grep and ctx_read find the notes without anyone
       having to parse JSON out of a script tag. A save replaces the last one rather than
       adding another, so saving never leaves a stale map behind for a grep to find. */
    var text = MIRROR_HEAD + '\n' + commentSafe(buildMap()) + '\n';
    var mirror = lastMirror(doc, island), body = doc.querySelector('body');
    if (mirror) mirror.data = text;
    else if (body) body.appendChild(document.createComment(text));

    /* Everything just written counts as seen, under the revision just stamped. */
    SYNC.notes = { rev: rev, seen: union(SYNC.notes.seen, idsOf('notes', N)) };
    SYNC.concurs = { rev: rev, seen: union(SYNC.concurs.seen, idsOf('concurs', C)) };
    try { localStorage.setItem(SKEY, JSON.stringify(SYNC)); } catch (e) {}

    var blob = new Blob(['<!DOCTYPE html>\n' + doc.outerHTML], { type: 'text/html' });
    var a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = FILE;
    a.click();
    setTimeout(function () { URL.revokeObjectURL(a.href); }, 4000);

    var b = document.getElementById('btn-embed');
    if (b) { var o = b.innerHTML; b.innerHTML = 'saved &#x2713;'; setTimeout(function () { b.innerHTML = o; }, 2200); }
  }

  function clearNotes() {
    if (!confirm('Delete every note in this brief?')) return;
    N = {}; save(); badges();
  }

  /* ---- wiring ---- */
  function init() {
    var ta = document.getElementById('note-text');
    if (ta) {
      ta.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); add(); }
      });
    }
    document.addEventListener('click', function (e) {
      var t = e.target.closest('[data-note-action]');
      if (!t) return;
      var a = t.dataset.noteAction;
      if (a === 'add') add();
      else if (a === 'export') exportNotes();
      else if (a === 'embed') embedNotes();
      else if (a === 'clear') clearNotes();
      else if (a === 'copy') copyExport(t);
    });
    ['d-note', 'd-export'].forEach(function (id) {
      var d = document.getElementById(id);
      if (!d) return;
      Array.prototype.forEach.call(d.querySelectorAll('[data-close]'), function (x) {
        x.addEventListener('click', function () { d.close(); });
      });
      d.addEventListener('click', function (e) { if (e.target === d) d.close(); });
    });
    tag();
    concurZones();
    badges();
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
