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
    warp();
  });

  /* ------------------------------------------------------------ barrel
     The warp-lab's approach 09: an SVG feDisplacementMap over a fixed tube,
     content scrolling inside it, and the filter dropped while the reader is
     actually scrolling so the scroll stays native-speed. The curve comes back
     180 ms after the last scroll event. */

  /* 0.26 was the first cut; Parker asked for half as much again. */
  var CURVE = 0.39;
  var built = '';

  function ensureFilter() {
    if (document.getElementById('td-barrel')) return;
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('width', '0'); svg.setAttribute('height', '0');
    svg.setAttribute('aria-hidden', 'true');
    svg.style.position = 'absolute';
    /* The region is the pane's own box, and the map is pinned to its origin.
       feImage defaults its subregion to the filter region, so a padded region
       (the warp lab used -8%) slides the map up and left by the padding, and
       every pixel it no longer covers is displaced by the full half-scale —
       which draws a second, offset copy of the page along the right and
       bottom edges. A barrel only ever samples inward, so no margin is
       needed. */
    svg.innerHTML =
      '<filter id="td-barrel" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">' +
      '<feImage id="td-barrel-map" x="0" y="0" result="map" preserveAspectRatio="none"/>' +
      '<feDisplacementMap id="td-barrel-disp" in="SourceGraphic" in2="map" xChannelSelector="R" yChannelSelector="G" scale="0"/>' +
      '</filter>';
    document.body.appendChild(svg);
  }

  function buildMap(W, H) {
    /* built at half size and stretched: the displacement field is smooth, so
       the saving is free and the toggle stays instant on a 4K window */
    var w = Math.max(2, Math.round(W / 2)), h = Math.max(2, Math.round(H / 2));
    var k1 = CURVE * 0.6, k2 = CURVE * 0.25;
    var cv = document.createElement('canvas'); cv.width = w; cv.height = h;
    var c = cv.getContext('2d'), img = c.createImageData(w, h);
    var cx = w / 2, cy = h / 2, ox = new Float32Array(w * h), oy = new Float32Array(w * h), mo = 1e-3;
    for (var y = 0; y < h; y++) for (var x = 0; x < w; x++) {
      var dx = (x - cx) / cx * 0.5, dy = (y - cy) / cy * 0.5, r2 = dx * dx + dy * dy, f = 1 + k1 * r2 + k2 * r2 * r2, i = y * w + x;
      ox[i] = dx * (f - 1) * W; oy[i] = dy * (f - 1) * H;
      if (Math.abs(ox[i]) > mo) mo = Math.abs(ox[i]);
      if (Math.abs(oy[i]) > mo) mo = Math.abs(oy[i]);
    }
    for (var j = 0; j < w * h; j++) {
      img.data[j * 4] = 128 + ox[j] / mo * 127; img.data[j * 4 + 1] = 128 + oy[j] / mo * 127;
      img.data[j * 4 + 2] = 128; img.data[j * 4 + 3] = 255;
    }
    c.putImageData(img, 0, 0);
    var url = cv.toDataURL();
    var map = document.getElementById('td-barrel-map');
    map.setAttribute('href', url);
    map.setAttributeNS('http://www.w3.org/1999/xlink', 'href', url);
    map.setAttribute('width', W); map.setAttribute('height', H);
    document.getElementById('td-barrel-disp').setAttribute('scale', (mo * 2).toFixed(2));
  }

  function warp() {
    if (!tube) return;
    var want = root.dataset.crt === 'on' && root.dataset.theme !== 'paper' && tube.clientWidth >= 600;
    if (!want) { tube.classList.remove('warp'); return; }
    ensureFilter();
    var key = tube.clientWidth + 'x' + tube.clientHeight;
    if (key !== built) { buildMap(tube.clientWidth, tube.clientHeight); built = key; }
    tube.classList.add('warp');
  }

  if (tube) {
    var tid = 0;
    var suspend = function () {
      if (!tube.classList.contains('warp')) return;
      tube.classList.add('scrolling');
      clearTimeout(tid);
      tid = setTimeout(function () { tube.classList.remove('scrolling'); }, 180);
    };
    tube.addEventListener('scroll', suspend, { passive: true });
    tube.addEventListener('wheel', suspend, { passive: true });
    var rt = 0;
    addEventListener('resize', function () { clearTimeout(rt); rt = setTimeout(warp, 160); });
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

  var REGS = { brief: 'r-brief', story: 'r-story', technical: 'r-technical' };
  function openFromHash() {
    var id = REGS[(location.hash || '').slice(1)];
    var r = id && document.getElementById(id);
    if (r) { r.checked = true; if (tube) tube.scrollTop = 0; }
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

  /* A command block carries its own copy button, which takes the commands
     and nothing else — no prompt, no comment header, no surrounding prose. */
  document.querySelectorAll('pre.cmd').forEach(function (pre) {
    var b = document.createElement('button');
    b.className = 'td-copy'; b.type = 'button'; b.setAttribute('data-copy-code', '');
    b.setAttribute('aria-label', 'Copy these commands');
    b.appendChild(document.createTextNode('Copy'));
    pre.appendChild(b);
  });
  document.addEventListener('click', function (e) {
    var b = e.target.closest('[data-copy-code]');
    if (!b) return;
    var code = b.closest('pre').querySelector('code');
    var text = (code || b.closest('pre')).textContent.trim();
    window.__tdLastCopy = text;
    var done = function () { b.classList.add('done'); b.lastChild.nodeValue = 'Copied'; setTimeout(function () { b.classList.remove('done'); b.lastChild.nodeValue = 'Copy'; }, 1400); };
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
     A prototype palette over the spine's page titles. Full-text search is a
     build step (Pagefind over the generated pages) and does not exist yet;
     the palette says so rather than pretending. */

  var pal = null;
  function openPalette() {
    if (!document.querySelector('.td-search')) return;
    if (!pal) {
      pal = document.createElement('div');
      pal.className = 'td-pal';
      pal.innerHTML = '<div class="td-pal-box" role="dialog" aria-label="Search the docs">' +
        '<input type="search" placeholder="Search page titles" aria-label="Search page titles">' +
        '<ul role="listbox"></ul><p class="td-pal-foot">Titles only for now. Full-text search arrives with the build.</p></div>';
      document.body.appendChild(pal);
      var input = pal.querySelector('input'), list = pal.querySelector('ul');
      var items = Array.prototype.map.call(document.querySelectorAll('.td-spine a'), function (a) {
        var g = a.closest('.td-group'); return { a: a, t: a.textContent.replace(/soon$/, '').trim(), g: g ? g.querySelector('h6').textContent : '' };
      });
      var render = function () {
        var q = input.value.trim().toLowerCase();
        list.innerHTML = '';
        items.filter(function (it) { return !q || (it.t + ' ' + it.g).toLowerCase().indexOf(q) >= 0; }).slice(0, 9).forEach(function (it, i) {
          var li = document.createElement('li');
          li.setAttribute('role', 'option'); if (i === 0) li.setAttribute('aria-selected', 'true');
          li.innerHTML = '<span></span><small></small>';
          li.firstChild.textContent = it.t; li.lastChild.textContent = it.g;
          li.addEventListener('click', function () { it.a.click(); closePalette(); });
          list.appendChild(li);
        });
      };
      input.addEventListener('input', render);
      input.addEventListener('keydown', function (e) {
        var cur = list.querySelector('[aria-selected="true"]');
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          var nx = cur && (e.key === 'ArrowDown' ? cur.nextElementSibling : cur.previousElementSibling);
          if (nx) { cur.removeAttribute('aria-selected'); nx.setAttribute('aria-selected', 'true'); }
        } else if (e.key === 'Enter' && cur) { cur.click(); }
      });
      pal.addEventListener('click', function (e) { if (e.target === pal) closePalette(); });
      render();
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
  warp();
  window.__tdShell = { warp: warp, copyRegister: copyRegister };
})();
