/* ==========================================================================
   The vanilla Terminal Delight shell — behaviour. See td-shell.css for the
   look. Nothing here is needed to read a page: with scripting off the page is
   glass or paper by the reader's OS setting, the registers still swap (they
   are radios), and every link still goes where it says.

   The mode is set before first paint by the small inline boot script each
   page carries in <head>; this file only wires the controls afterwards.
   ========================================================================== */
(function () {
  'use strict';

  var KEY = 'td-shell';
  var root = document.documentElement;
  var tube = document.getElementById('tube');
  var REDUCED = matchMedia('(prefers-reduced-motion: reduce)').matches;

  function prefs() { try { return JSON.parse(localStorage.getItem(KEY)) || {}; } catch (e) { return {}; } }
  function save(k, v) { var p = prefs(); p[k] = v; try { localStorage.setItem(KEY, JSON.stringify(p)); } catch (e) {} }

  /* ---------------------------------------------------- mode + texture */

  function syncControls() {
    document.querySelectorAll('[data-td-toggle="crt"]').forEach(function (b) {
      var on = root.dataset.crt === 'on';
      b.setAttribute('aria-pressed', on ? 'true' : 'false');
      b.title = root.dataset.theme === 'paper'
        ? (on ? 'Green-bar paper: on' : 'Green-bar paper: off')
        : (on ? 'CRT glass: on' : 'CRT glass: off');
    });
    document.querySelectorAll('[data-td-toggle="theme"]').forEach(function (b) {
      b.title = root.dataset.theme === 'paper' ? 'Switch to glass (dark)' : 'Switch to paper (light)';
    });
  }

  document.addEventListener('click', function (e) {
    var t = e.target.closest('[data-td-toggle]');
    if (!t) return;
    if (t.dataset.tdToggle === 'theme') {
      root.dataset.theme = root.dataset.theme === 'paper' ? 'glass' : 'paper';
      save('theme', root.dataset.theme);
    } else if (t.dataset.tdToggle === 'crt') {
      root.dataset.crt = root.dataset.crt === 'on' ? 'off' : 'on';
      save('crt', root.dataset.crt);
    }
    syncControls();
    glass();
  });

  /* ------------------------------------------------------------- glass
     Warp the glass, never the text.

     The first version bent the page itself with an SVG feDisplacementMap,
     and at the curve Parker wanted it demolished the text: Chrome samples
     that filter nearest-pixel at screen resolution, so every glyph, rule and
     card border breaks wherever the displacement crosses a whole pixel.
     Laying the page out at twice the size and scaling it back does not help
     — the filter is still computed at screen size (tested 2026-09-25).
     curved-glass-web had already written the rule down: "Warp the GLASS,
     never the live text... the eye reads CRT mostly from scanlines +
     vignette + shadow mask" (docs/LESSONS.md).

     So the page under the tube stays flat, and the glass on top of it is
     bent through the same barrel: scanlines that bow along the curve, a
     cushion-shaped screen edge with rounded corners, and a shadow inside the
     rim. It is drawn once per size into one canvas, costs nothing to scroll,
     and resamples no text. */

  /* 0.26 was the first cut; Parker asked for half as much again. */
  var CURVE = 0.39;
  var SCAN_PERIOD = 4;       // CSS px between scanlines, as the flat overlay had
  var RIM = 22;              // CSS px of shadow inside the curved edge
  var drawn = '';

  function glassCanvas() {
    var fx = document.getElementById('td-fx');
    if (!fx) return null;
    var cv = fx.querySelector('canvas.glass');
    if (!cv) {
      cv = document.createElement('canvas');
      cv.className = 'glass';
      cv.setAttribute('aria-hidden', 'true');
      fx.insertBefore(cv, fx.firstChild);
      fx.classList.add('curved');
    }
    return cv;
  }

  function drawGlass(cv, W, H) {
    var dpr = Math.min(window.devicePixelRatio || 1, 2);
    var w = Math.max(2, Math.round(W * dpr)), h = Math.max(2, Math.round(H * dpr));
    cv.width = w; cv.height = h;
    var c = cv.getContext('2d'), img = c.createImageData(w, h), d = img.data;
    var k1 = CURVE * 0.6, k2 = CURVE * 0.25;
    var period = SCAN_PERIOD * dpr, rim = RIM * dpr;
    for (var y = 0; y < h; y++) {
      var ny = y / h - 0.5;
      for (var x = 0; x < w; x++) {
        var nx = x / w - 0.5, r2 = nx * nx + ny * ny, f = 1 + k1 * r2 + k2 * r2 * r2;
        /* where this screen pixel would sample from under the barrel */
        var sx = (0.5 + nx * f) * w, sy = (0.5 + ny * f) * h;
        /* distance, in device px, to the edge of the bent screen */
        var edge = Math.min(sx, w - sx, sy, h - sy);
        var a;
        if (edge <= 0) a = Math.min(1, 0.5 - edge);                 // outside the glass: black
        else {
          var line = sy % period;                                  // a scanline, bowed with the curve
          a = line < dpr ? 0.2 : 0;
          if (edge < rim) { var t = 1 - edge / rim; a = Math.max(a, 0.55 * t * t); } // shadow in the rim
          if (edge < 1) a = Math.max(a, 1 - edge);                 // antialias the edge
        }
        var i = (y * w + x) * 4;
        d[i] = 0; d[i + 1] = 0; d[i + 2] = 0; d[i + 3] = a * 255;
      }
    }
    c.putImageData(img, 0, 0);
  }

  function glass() {
    if (!tube) return;
    var on = root.dataset.crt === 'on' && root.dataset.theme !== 'paper';
    var cv = glassCanvas();
    if (!cv) return;
    if (!on) { cv.hidden = true; return; }
    cv.hidden = false;
    var key = tube.clientWidth + 'x' + tube.clientHeight + '@' + (window.devicePixelRatio || 1);
    if (key !== drawn) { drawGlass(cv, tube.clientWidth, tube.clientHeight); drawn = key; }
  }

  if (tube) {
    var rt = 0;
    addEventListener('resize', function () { clearTimeout(rt); rt = setTimeout(glass, 160); });
  }

  /* ----------------------------------------------------- spine drawer */

  document.addEventListener('click', function (e) {
    if (e.target.closest('[data-td-menu]')) { root.classList.toggle('spine-open'); return; }
    if (root.classList.contains('spine-open') && (e.target.closest('.td-spine a') || !e.target.closest('.td-spine'))) {
      root.classList.remove('spine-open');
    }
  });

  /* --------------------------------------------- where-am-I on the spine
     Spine links that point at a section of this page light up as the section
     crosses the upper third of the tube. */

  var spy = Array.prototype.filter.call(document.querySelectorAll('.td-spine a[href^="#"]'), function (a) {
    return a.getAttribute('href').length > 1 && document.getElementById(a.getAttribute('href').slice(1));
  });
  if (spy.length && tube && 'IntersectionObserver' in window) {
    var byId = {};
    spy.forEach(function (a) { byId[a.getAttribute('href').slice(1)] = a; });
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (!en.isIntersecting) return;
        spy.forEach(function (a) { a.classList.remove('is-here'); });
        var a = byId[en.target.id]; if (a) a.classList.add('is-here');
      });
    }, { root: tube, rootMargin: '-30% 0px -62% 0px' });
    Object.keys(byId).forEach(function (id) { io.observe(document.getElementById(id)); });
  }

  /* -------------------------------------------------------- registers */

  /* A hash names a register (#technical), or anything inside one
     (#technical-keys): open the register that holds it, then go there. */
  var REGS = { brief: 'r-brief', story: 'r-story', technical: 'r-technical' };
  function openFromHash() {
    var h = decodeURIComponent((location.hash || '').slice(1));
    if (!h) return;
    if (REGS[h]) {
      var r = document.getElementById(REGS[h]);
      if (r) { r.checked = true; if (tube) tube.scrollTop = 0; }
      return;
    }
    var el = document.getElementById(h);
    if (!el) return;
    var panel = el.closest('.reg-panel'), radio = panel && REGS[panel.id] && document.getElementById(REGS[panel.id]);
    if (radio) radio.checked = true;
    requestAnimationFrame(function () { el.scrollIntoView({ block: 'start' }); });
  }
  openFromHash();
  addEventListener('hashchange', openFromHash);
  document.querySelectorAll('.reg > input[name="register"]').forEach(function (r) {
    r.addEventListener('change', function () {
      var name = Object.keys(REGS).filter(function (k) { return REGS[k] === r.id; })[0];
      if (name) history.replaceState(null, '', '#' + name);
    });
  });

  /* Copy, scoped. The button is handed its own register panel and converts
     that alone; it never walks up to the article and back down, which is the
     defect that made a copy on the BFS articles return every register at
     once. */

  function toText(node) {
    var out = [];
    function walk(n) {
      if (n.nodeType === 3) { out.push(n.nodeValue.replace(/\s+/g, ' ')); return; }
      if (n.nodeType !== 1) return;
      if (n.matches('.reg-head, .td-copy, script, style, [aria-hidden="true"], svg')) return;
      var tag = n.tagName;
      if (/^H[1-6]$/.test(tag)) { out.push('\n\n' + n.textContent.trim() + '\n\n'); return; }
      if (tag === 'A') { out.push(n.textContent.trim() + ' (' + n.href + ')'); return; }
      if (tag === 'LI') { out.push('\n- '); n.childNodes.forEach(walk); return; }
      if (tag === 'TR') { out.push('\n' + Array.prototype.map.call(n.cells, function (c) { return c.textContent.trim(); }).join(' | ')); return; }
      if (tag === 'PRE') { var c = n.querySelector('code'); out.push('\n\n' + (c || n).textContent.replace(/\n$/, '') + '\n\n'); return; }
      if (tag === 'BR') { out.push('\n'); return; }
      var block = /^(P|DIV|SECTION|FIGURE|FIGCAPTION|UL|OL|TABLE|BLOCKQUOTE)$/.test(tag);
      if (block) out.push('\n\n');
      n.childNodes.forEach(walk);
      if (block) out.push('\n\n');
    }
    walk(node);
    return out.join('').replace(/[ \t]+\n/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
  }

  function copyRegister(panel, btn) {
    var title = (document.querySelector('.td-doc h1') || {}).textContent || document.title;
    var label = panel.getAttribute('data-register') || '';
    var head = [
      title.trim(),
      'Source: Terminal Delight docs',
      'Register: ' + label,
      'Updated: ' + (document.querySelector('meta[name="td:updated"]') || {}).content,
      'Source URL: ' + location.href.split('#')[0] + '#' + panel.id,
      '', ''
    ].join('\n');
    var text = head + toText(panel);
    var done = function () {
      if (!btn) return;
      var was = btn.lastChild.nodeValue;
      btn.classList.add('done'); btn.lastChild.nodeValue = ' Copied';
      setTimeout(function () { btn.classList.remove('done'); btn.lastChild.nodeValue = was; }, 1400);
    };
    if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(done, done);
    window.__tdLastCopy = text;
  }

  /* A command block carries one copy button, which takes the commands and
     nothing else — no prompt, no comment header, no surrounding prose — as ONE
     line, the commands joined with && so a single paste runs them in order and
     stops at the first failure. Copying is said once, by the whole block
     throbbing, rather than by a second label beside the button. */
  document.querySelectorAll('pre.cmd').forEach(function (pre) {
    var b = document.createElement('button');
    b.className = 'td-copy'; b.type = 'button'; b.setAttribute('data-copy-code', '');
    b.setAttribute('aria-label', 'Copy these commands as one line');
    b.appendChild(document.createTextNode('⎘ copy'));
    pre.appendChild(b);
  });
  document.addEventListener('click', function (e) {
    var b = e.target.closest('[data-copy-code]');
    if (!b) return;
    var pre = b.closest('pre'), code = pre.querySelector('code');
    /* comment lines are dropped: joined into one line, a # would silence every command after it */
    var text = (code || pre).textContent.split('\n').map(function (l) { return l.trim(); })
      .filter(function (l) { return l && l.charAt(0) !== '#'; }).join(' && ');
    window.__tdLastCopy = text;
    var done = function () {
      pre.classList.remove('throb'); void pre.offsetWidth; pre.classList.add('throb');
      b.classList.add('done'); b.lastChild.nodeValue = '✓ copied';
      setTimeout(function () { pre.classList.remove('throb'); b.classList.remove('done'); b.lastChild.nodeValue = '⎘ copy'; }, 1400);
    };
    if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(done, done); else done();
  });

  document.addEventListener('click', function (e) {
    var b = e.target.closest('[data-copy]');
    if (!b) return;
    var panel = b.dataset.copy === 'visible' ? visiblePanel() : b.closest('.reg-panel');
    if (panel) copyRegister(panel, b);
  });
  function visiblePanel() {
    var list = document.querySelectorAll('.reg-panel');
    for (var i = 0; i < list.length; i++) if (list[i].offsetParent !== null) return list[i];
    return null;
  }

  /* ---------------------------------------------------------- search
     Full text, from the search.json the docs build writes: one entry per
     heading of every register of every page. A query matches when every word
     in it appears in the entry's page title, heading or text; titles and
     headings count for more than body text. With the index missing (a page
     opened from disk, a failed fetch) the palette searches the spine's page
     titles instead, and says so. */

  var pal = null, index = null, indexTried = false;
  function loadIndex(then) {
    if (index || indexTried) { then(); return; }
    indexTried = true;
    fetch('/search.json').then(function (r) { if (!r.ok) throw 0; return r.json(); })
      .then(function (d) { index = d; then(); }, function () { then(); });
  }
  function titlesOnly() {
    return Array.prototype.map.call(document.querySelectorAll('.td-spine a'), function (a) {
      var g = a.closest('.td-group');
      return { u: a.getAttribute('href'), t: a.textContent.trim(), g: g ? g.querySelector('h6').textContent : '', r: '', s: '', a: '', x: '' };
    });
  }
  function score(e, words, phrase) {
    var t = e.t.toLowerCase(), s = (e.s || '').toLowerCase(), x = (e.x || '').toLowerCase(), n = 0;
    for (var i = 0; i < words.length; i++) {
      var w = words[i], hit = false;
      if (t.indexOf(w) >= 0) { n += 10; hit = true; }
      if (s.indexOf(w) >= 0) { n += 6; hit = true; }
      if (x.indexOf(w) >= 0) { n += 1; hit = true; }
      if (!hit) return 0;
    }
    if (phrase && (t + ' ' + s + ' ' + x).indexOf(phrase) >= 0) n += 4;
    return n;
  }
  function snippet(x, words) {
    if (!x) return '';
    var lx = x.toLowerCase(), at = -1;
    for (var i = 0; i < words.length && at < 0; i++) at = lx.indexOf(words[i]);
    var start = Math.max(0, at - 40), piece = (start ? '…' : '') + x.slice(start, start + 160);
    var safe = piece.replace(/&/g, '&amp;').replace(/</g, '&lt;');
    words.forEach(function (w) {
      if (w.length < 2) return;
      safe = safe.replace(new RegExp('(' + w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'ig'), '<mark>$1</mark>');
    });
    return safe;
  }
  function openPalette() {
    if (!document.querySelector('.td-search')) return;
    if (!pal) {
      pal = document.createElement('div');
      pal.className = 'td-pal';
      pal.innerHTML = '<div class="td-pal-box" role="dialog" aria-label="Search the docs">' +
        '<input type="search" placeholder="Search the docs" aria-label="Search the docs">' +
        '<ul role="listbox"></ul><p class="td-pal-foot"></p></div>';
      document.body.appendChild(pal);
      var input = pal.querySelector('input'), list = pal.querySelector('ul'), foot = pal.querySelector('.td-pal-foot');
      var render = function () {
        var raw = input.value.trim().toLowerCase(), words = raw.split(/\s+/).filter(Boolean);
        var all = index || titlesOnly();
        foot.textContent = index ? 'Every page, full text. ↑ ↓ to move, Enter to open.' : 'Page titles only: the search index did not load.';
        var hits;
        if (!words.length) hits = all.filter(function (e) { return !e.s; }).slice(0, 12).map(function (e) { return { e: e, n: 1 }; });
        else hits = all.map(function (e) { return { e: e, n: score(e, words, raw) }; }).filter(function (h) { return h.n > 0; })
          .sort(function (a, b) { return b.n - a.n; }).slice(0, 12);
        list.innerHTML = '';
        hits.forEach(function (h, i) {
          var e = h.e, li = document.createElement('li');
          li.setAttribute('role', 'option'); if (i === 0) li.setAttribute('aria-selected', 'true');
          li.innerHTML = '<span></span><small></small>' + (words.length && e.x ? '<span class="snip"></span>' : '');
          li.firstChild.textContent = e.t + (e.s ? ' › ' + e.s : '');
          li.children[1].textContent = e.r ? e.g + ' · ' + e.r : e.g;
          if (li.children[2]) li.children[2].innerHTML = snippet(e.x, words);
          li.addEventListener('click', function () {
            var dest = e.u + (e.a ? '#' + e.a : '');
            closePalette();
            if (dest.split('#')[0] === location.pathname && e.a) { location.hash = e.a; openFromHash(); }
            else location.href = dest;
          });
          list.appendChild(li);
        });
        if (!hits.length) list.innerHTML = '<li aria-disabled="true"><span>Nothing matches every word.</span></li>';
      };
      input.addEventListener('input', render);
      loadIndex(render);
      input.addEventListener('keydown', function (e) {
        var cur = list.querySelector('[aria-selected="true"]');
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          var nx = cur && (e.key === 'ArrowDown' ? cur.nextElementSibling : cur.previousElementSibling);
          if (nx) { cur.removeAttribute('aria-selected'); nx.setAttribute('aria-selected', 'true'); }
        } else if (e.key === 'Enter' && cur) { cur.click(); }
      });
      pal.addEventListener('click', function (e) { if (e.target === pal) closePalette(); });
    }
    pal.classList.add('open');
    pal.querySelector('input').focus();
  }
  function closePalette() { if (pal) pal.classList.remove('open'); }
  document.addEventListener('click', function (e) { if (e.target.closest('.td-search')) openPalette(); });
  document.addEventListener('keydown', function (e) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); openPalette(); }
    else if (e.key === 'Escape') { closePalette(); root.classList.remove('spine-open'); }
  });

  syncControls();
  glass();
  window.__tdShell = { glass: glass, copyRegister: copyRegister };
})();
