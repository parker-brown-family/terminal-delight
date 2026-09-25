/* ==========================================================================
   The docs' own furniture, on top of td-shell.js (which keeps the glass and
   paper switch, the CRT switch, the registers, copy and search) and
   td-glass.js (the tube). This file adds what only the docs have:

     the theme       one of the Omarchy palettes kiosk-theme.js carries,
                     painted into <style id="td-palette"> by the boot script
                     in <head> and repainted here. A <style>, not inline
                     properties, because the tube bends a snapshot of the page
                     and a snapshot reads stylesheets, not the style attribute.
     the wallpaper   each theme's own Omarchy wallpaper behind the page,
                     crossfaded when the theme changes, off with w.
     the tray        the picker, after the Omarchy kiosk's rail: one glass
                     pill per theme, with a line drawing of it.
     the keys        one letter for everything a reader does here, and ? to
                     list them.
   ========================================================================== */
(function () {
  'use strict';
  var K = window.TD_KIOSK;
  var root = document.documentElement;
  if (!K) return;

  var WALLS = '/assets/omarchy/bg/';
  var PREFS = 'td-shell';
  function prefs() { try { return JSON.parse(localStorage.getItem(PREFS)) || {}; } catch (e) { return {}; } }
  function save(k, v) { var p = prefs(); p[k] = v; try { localStorage.setItem(PREFS, JSON.stringify(p)); } catch (e) {} }
  var $ = function (id) { return document.getElementById(id); };
  var esc = function (s) { return String(s).replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); };

  /* One line drawing per theme, stroked in the current ink. */
  var G = function (d) { return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + d + '</svg>'; };
  var GLYPH = {
    'last-voyage': G('<path d="M12 3v13"/><path d="M12 4.5 18 15h-6"/><path d="M11 7 6.5 15H11"/><path d="M3 17.5h18l-2.2 3H5.2z"/>'),
    'gruvbox': G('<rect x="3" y="9" width="18" height="11" rx="2"/><path d="M7 9 16.5 3.5"/><circle cx="15.5" cy="14.5" r="3"/><path d="M6 12.5h4M6 15h4M6 17.5h4"/>'),
    'osaka-jade': G('<path d="M2.5 6.5Q12 3.5 21.5 6.5"/><path d="M4.5 9.5h15"/><path d="M7 6v15M17 6v15"/><path d="M12 9.5v-3"/>'),
    'tokyo-night': G('<path d="M3 21v-7h4v7M7 21V9.5h5V21M12 21v-5h4v5M16 21v-9h5v9"/><path d="M18.5 2.5a3.2 3.2 0 1 0 3 4.6 3.6 3.6 0 0 1-3-4.6z"/>'),
    'ethereal': G('<path d="M11 3.5 12.8 8.7 18 10.5 12.8 12.3 11 17.5 9.2 12.3 4 10.5 9.2 8.7z"/><path d="M18.5 15.5l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8z"/>'),
    'everforest': G('<path d="M12 2.5 17 9.5h-3l4.5 6.5h-13L10 9.5H7z"/><path d="M12 16v5.5"/>'),
    'miasma': G('<path d="M3.5 12.5a8.5 7.5 0 0 1 17 0z"/><path d="M9 12.5v5.5a3 3 0 0 0 6 0v-5.5"/><circle cx="9" cy="8.5" r="1"/><circle cx="14.8" cy="7.6" r="1.2"/>'),
    'retro-82': G('<rect x="3.5" y="15" width="17" height="6" rx="2"/><path d="M12 15V9"/><circle cx="12" cy="6.2" r="3"/><path d="M16.5 18h.01"/>'),
  };
  var glyph = function (name) { return GLYPH[name] || G('<rect x="4" y="4" width="16" height="16" rx="3"/>'); };

  /* ------------------------------------------------------------- the theme */
  var current = K.byName(root.dataset.palette) || K.resolve().palette;
  function paint(p) {
    var el = $('td-palette');
    if (!el) { el = document.createElement('style'); el.id = 'td-palette'; document.head.appendChild(el); }
    el.textContent = ':root{' + K.VARS.map(function (k) { return '--' + k + ':' + p[k]; }).join(';') + '}';
    root.dataset.palette = p.name;
  }

  /* ---------------------------------------------------------- the wallpaper */
  var back = document.createElement('div');
  back.className = 'backdrop'; back.setAttribute('aria-hidden', 'true');
  back.innerHTML = '<div class="layer"></div><div class="layer"></div>';
  document.body.insertBefore(back, document.body.firstChild);
  var layers = back.querySelectorAll('.layer'), wallOn = 0, wallImg = null;
  function wallpaper(name) {
    var cur = layers[wallOn], next = layers[1 - wallOn], url = WALLS + name + '.webp';
    if (cur.dataset.name === name) return;
    var img = new Image();
    img.onload = function () {
      next.style.backgroundImage = 'url("' + url + '")'; next.dataset.name = name;
      next.classList.add('on'); cur.classList.remove('on'); wallOn = 1 - wallOn;
      wallImg = img; markWall();
    };
    img.src = url;
  }
  function setWall(on) {
    root.dataset.wall = on ? 'on' : 'off'; save('wall', root.dataset.wall);
    var b = $('td-wallbtn'); if (b) b.setAttribute('aria-pressed', String(on));
  }

  /* The wall, handed to the tube. With CRT on, td-glass.js draws the page from
     a snapshot, and a snapshot cannot hold what is behind the page, so the
     wall is drawn here the way the backdrop draws it — the theme's wallpaper
     covering the box, blurred, under the scrim of the theme's own ground — and
     the tube composites the page over it. map() places the pane on the wall
     from the backdrop's live box, which already carries the scroll's growth
     and pan. Drawn only while the tube is wanted, and again whenever the
     theme, the wallpaper switch or the window's size changes it. */
  var wallCanvas = document.createElement('canvas'), wallDirty = true;
  var glassWall = {
    canvas: wallCanvas, version: 0,
    map: function (r) {
      var b = back.getBoundingClientRect();
      return [r.width / b.width, r.height / b.height, (r.left - b.left) / b.width, (r.top - b.top) / b.height];
    },
  };
  var tubeWanted = function () { return root.dataset.crt === 'on' && root.dataset.theme !== 'paper'; };
  function drawWall() {
    wallDirty = false;
    var d = Math.min(window.devicePixelRatio || 1, 1.5), W = back.offsetWidth, H = back.offsetHeight;
    if (!W || !H) return;
    wallCanvas.width = Math.round(W * d); wallCanvas.height = Math.round(H * d);
    var g = wallCanvas.getContext('2d'), cs = getComputedStyle(root);
    var ground = cs.getPropertyValue('--bg').trim() || '#000';
    g.fillStyle = ground; g.fillRect(0, 0, wallCanvas.width, wallCanvas.height);
    if (root.dataset.wall !== 'off' && wallImg) {
      /* background: center / cover, on a layer 12px larger than the box all round */
      var lw = (W + 24) * d, lh = (H + 24) * d, ir = wallImg.naturalWidth / wallImg.naturalHeight;
      var dw = ir > lw / lh ? lh * ir : lw, dh = ir > lw / lh ? lh : lw / ir;
      g.filter = 'blur(' + (2.5 * d).toFixed(2) + 'px) saturate(1.08)';
      g.drawImage(wallImg, -12 * d + (lw - dw) / 2, -12 * d + (lh - dh) / 2, dw, dh);
      g.filter = 'none';
      g.globalAlpha = parseFloat(cs.getPropertyValue('--scrim')) || 0.8;
      g.fillStyle = ground; g.fillRect(0, 0, wallCanvas.width, wallCanvas.height);
      g.globalAlpha = 1;
    }
    glassWall.version++;
  }
  function markWall() { wallDirty = true; if (tubeWanted()) drawWall(); }
  window.TD_GLASS_WALL = glassWall;
  new MutationObserver(markWall).observe(root, { attributes: true, attributeFilter: ['data-crt', 'data-theme', 'data-palette', 'data-wall'] });
  var wt = 0;
  addEventListener('resize', function () { clearTimeout(wt); wt = setTimeout(markWall, 150); });

  /* -------------------------------------------------------------- the tray */
  var btn = document.querySelector('.td-themebtn');
  var shelf = document.createElement('div');
  shelf.innerHTML =
    '<div class="td-scrim" id="td-scrim"></div>' +
    '<div class="td-pop td-tray" id="td-tray" role="menu" aria-label="Theme">' +
      '<div class="th">▸ <span id="td-traynow"></span><span class="k">t · ↑↓ · ↵</span></div>' +
      '<button class="td-wallbtn" id="td-wallbtn" type="button" aria-pressed="true" title="Wallpaper (w)"><span class="dot"></span>wallpaper</button>' +
      '<div class="td-traylist" id="td-traylist"></div>' +
    '</div>' +
    '<div class="td-pop td-keys" id="td-keys" role="dialog" aria-label="Keys"><h3>▸ TERMINAL DELIGHT DOCS · KEYS</h3><div class="cols">' +
      '<div><h4>These pages</h4><dl>' +
        '<dt>Ctrl K · /</dt><dd>Search every page</dd><dt>[ · ]</dt><dd>Previous · next page</dd>' +
        '<dt>1 · 2 · 3</dt><dd>Brief · Story · Technical</dd><dt>k</dt><dd>Every key in the app</dd><dt>t</dt><dd>Theme tray</dd><dt>w</dt><dd>Wallpaper on or off</dd>' +
        '<dt>c</dt><dd>CRT glass on or off</dd><dt>m</dt><dd>Paper or glass</dd><dt>?</dt><dd>This list</dd><dt>Esc</dt><dd>Close whatever is open</dd></dl></div>' +
      '<div><h4>In Terminal Delight</h4><dl>' +
        '<dt>F1</dt><dd>Every shortcut, and the language</dd>' +
        '<dt>Ctrl+Shift+T</dt><dd>New tab</dd><dt>Alt+V · Alt+H</dt><dd>Split beside · below</dd><dt>Alt + arrows</dt><dd>Move between panes</dd>' +
        '<dt>Alt+W</dt><dd>Close a pane</dd><dt>Ctrl+Shift+Z</dt><dd>Bring back what you closed</dd><dt>Alt+K</dt><dd>Terminal ⇄ workbench</dd>' +
        '<dt>Ctrl+Shift+N</dt><dd>Which agent needs you</dd><dt>Alt+R</dt><dd>Read this pane big</dd></dl></div>' +
    '</div><div class="kf">Every key in the app: <a href="/keys" data-sheet>the full sheet →</a> · Esc to close</div></div>';
  while (shelf.firstChild) document.body.appendChild(shelf.firstChild);

  var open = null, hl = 0;
  function show(id) {
    hide(); open = id; $(id).classList.add('on'); $('td-scrim').classList.add('on');
    if (id === 'td-keys' && keysBtn) keysBtn.setAttribute('aria-expanded', 'true');
    if (id === 'td-tray') { if (btn) btn.setAttribute('aria-expanded', 'true'); hl = K.PALETTES.indexOf(current); drawTray(); }
  }
  function hide() {
    if (!open) return;
    $(open).classList.remove('on'); $('td-scrim').classList.remove('on');
    if (btn) btn.setAttribute('aria-expanded', 'false');
    if (keysBtn) keysBtn.setAttribute('aria-expanded', 'false');
    open = null;
  }
  function drawTray() {
    $('td-traylist').innerHTML = K.PALETTES.map(function (p, i) {
      return '<button class="td-chip' + (i === hl ? ' hl' : '') + '" type="button" role="menuitemradio" aria-checked="' + (p.name === current.name) + '" data-i="' + i + '">' +
        '<span class="pie" style="background:conic-gradient(' + p.acc + ' 0 25%,' + p.grn + ' 0 50%,' + p.mag + ' 0 75%,' + p.yel + ' 0)"></span>' +
        '<span>' + esc(p.name) + '</span>' + glyph(p.name) + '</button>';
    }).join('');
  }
  function setTheme(p, keep) {
    current = p;
    if (keep) K.remember(p.name);
    paint(p); wallpaper(p.name);
    if (btn) {
      btn.querySelector('.gl').innerHTML = glyph(p.name);
      btn.querySelector('.nm').textContent = p.name;
      btn.title = 'Theme: ' + p.name + ' (t)';
    }
    $('td-traynow').textContent = p.name;
  }
  function pick(i) { setTheme(K.PALETTES[i], true); hide(); }

  if (btn) btn.addEventListener('click', function () { open === 'td-tray' ? hide() : show('td-tray'); });
  $('td-traylist').addEventListener('click', function (e) { var b = e.target.closest('button[data-i]'); if (b) pick(+b.dataset.i); });
  $('td-wallbtn').addEventListener('click', function () { setWall(root.dataset.wall === 'off'); });
  $('td-scrim').addEventListener('click', hide);
  /* the keyboard glyph beside the CRT: the same list ? opens */
  var keysBtn = document.querySelector('.td-keysbtn');
  if (keysBtn) keysBtn.addEventListener('click', function () { open === 'td-keys' ? hide() : show('td-keys'); });
  /* The Keymapping tab, and any link marked data-sheet, open the app's whole
     key sheet (written into the page by the build, from the app's own source).
     A middle click, a modified click, or no script at all still goes to /keys. */
  document.addEventListener('click', function (e) {
    var a = e.target.closest('[data-sheet]');
    if (!a || !$('td-sheet') || e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    open === 'td-sheet' ? hide() : show('td-sheet');
  });

  /* -------------------------------------------------------------- the keys */
  function press(sel) { var el = document.querySelector(sel); if (el) el.click(); }
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') { hide(); return; }
    if (open === 'td-tray') {
      var n = K.PALETTES.length;
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); hl = (hl + (e.key === 'ArrowDown' ? 1 : -1) + n) % n; drawTray(); return; }
      if (e.key === 'Enter') { e.preventDefault(); pick(hl); return; }
    }
    var a = document.activeElement;
    if (e.ctrlKey || e.metaKey || e.altKey || (a && /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName) && a.type !== 'radio') || (a && a.isContentEditable)) return;
    var k = e.key;
    if (k === '/') { e.preventDefault(); hide(); press('.td-search'); }
    else if (k === '?') { open === 'td-keys' ? hide() : show('td-keys'); }
    else if (k === 't') { open === 'td-tray' ? hide() : show('td-tray'); }
    else if (k === 'k' && $('td-sheet')) { open === 'td-sheet' ? hide() : show('td-sheet'); }
    else if (k === 'w') setWall(root.dataset.wall === 'off');
    else if (k === 'c') press('[data-td-toggle="crt"]');
    else if (k === 'm') press('[data-td-toggle="theme"]');
    else if (k === '[') press('.td-pager a.prev');
    else if (k === ']') press('.td-pager a.next');
    else if (/^[123]$/.test(k)) {
      var r = document.querySelectorAll('.reg > input[name="register"]')[+k - 1];
      if (r && !r.checked) { r.checked = true; r.dispatchEvent(new Event('change', { bubbles: true })); }
    }
  });

  /* ------------------------------------------------ the scroll choreography
     From the wellness-with-kate build, by way of the Omarchy kiosk: the wall
     behind the page grows, and the view pans down it, as you scroll, and each block of the
     page rolls into focus as it passes through a band of the pane, the title
     drifting up and away as you leave it. The kiosk's gentle numbers, because
     this is text being read, with the band raised so a heading you jump to
     lands in focus, and released over the last half-screen so the end of a
     page is a place to arrive at.

     The pane is #tube, not the window. With the tube on, the blocks stand
     still: the tube bends one snapshot of the whole page, and a style written
     into it on every frame would ask for a new snapshot on every frame. The
     wall is outside the tube and keeps growing either way. */
  var tube = document.getElementById('tube');
  /* 40% of growth and 42vh of pan over a whole page: the box starts 6vh past
     the screen's foot and ends 1.06 + 1.12 × 0.4 − 0.42 = 1.09 screens down */
  var ZOOM = 0.4, PAN = 42;
  var BLOCKS = '.reg-tabs, .reg-panel > *, .plain > *, .td-doc > .src-head, .td-doc > .src, .td-doc > .td-pager, .td-doc > .td-foot';
  var tubed = function () { return root.dataset.crt === 'on' && root.dataset.theme !== 'paper'; };
  var ticking = false, still = true;
  function frame() {
    ticking = false;
    var y = tube.scrollTop, vh = tube.clientHeight, max = Math.max(0, tube.scrollHeight - vh);
    var p = max > 0 ? Math.min(1, y / max) : 0;
    back.style.setProperty('--s', (1 + p * ZOOM).toFixed(4));
    back.style.setProperty('--pan', (p * PAN).toFixed(2));
    if (tubed()) { if (!still) settle(); return; }
    still = false;
    var t0 = tube.getBoundingClientRect().top + tube.clientTop;
    var top = t0 + vh * 0.04, bot = t0 + vh * 0.9, band = bot - top;
    var release = max <= 0 ? 1 : Math.max(0, Math.min(1, 1 - (max - y) / (vh * 0.5)));
    var list = tube.querySelectorAll(BLOCKS);
    for (var i = 0; i < list.length; i++) {
      var el = list[i], r = el.getBoundingClientRect();
      if (!r.height || r.bottom < t0 - vh || r.top > t0 + 2 * vh) continue;
      /* a block is in focus once a third of a screen of it, or all of it, is in the band,
         so a tall table does not sit dimmed in the middle of the pane while you read it */
      var f = Math.max(0, Math.min(1, (Math.min(r.bottom, bot) - Math.max(r.top, top)) / (Math.min(band, r.height, vh * 0.33) || 1)));
      var v = (f + (1 - f) * release).toFixed(3);
      if (el.style.getPropertyValue('--f') !== v) el.style.setProperty('--f', v);
      if (!el.classList.contains('choreo')) el.classList.add('choreo');
      el.classList.toggle('out', v < 0.999);
    }
    var h1 = tube.querySelector('.td-doc > h1');
    if (h1) h1.style.setProperty('--p', Math.min(1, y / (vh * 0.6)).toFixed(3));
  }
  /* back to plain blocks, once, when the tube comes on */
  function settle() {
    still = true;
    tube.querySelectorAll('.choreo').forEach(function (el) { el.classList.remove('choreo', 'out'); el.style.removeProperty('--f'); });
    var h1 = tube.querySelector('.td-doc > h1'); if (h1) h1.style.removeProperty('--p');
  }
  function onScroll() { if (!ticking) { ticking = true; requestAnimationFrame(frame); } }
  if (tube && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
    root.classList.add('choreo-on');
    tube.addEventListener('scroll', onScroll, { passive: true });
    addEventListener('resize', onScroll, { passive: true });
    /* a register switch, the tube or paper, a theme: the blocks on screen change */
    document.addEventListener('change', onScroll);
    new MutationObserver(onScroll).observe(root, { attributes: true, attributeFilter: ['data-crt', 'data-theme'] });
    frame();
  }

  /* -------------------------------------------------------------- boot */
  setWall(root.dataset.wall !== 'off');
  setTheme(current, false);
  window.__tdDocs = { setTheme: function (name) { var p = K.byName(name); if (p) setTheme(p, true); }, current: function () { return current.name; } };
})();
