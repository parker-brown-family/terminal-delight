/* Docs redesign prototype — the chrome both pages share: header, tabs and menus,
   the left bar, the theme tray and wallpaper, search, copy, register tabs and keys.
   Theme colours come from /assets/kiosk-theme.js (TD_KIOSK), so a pick made on any
   kiosk is the pick here too, and each theme wears its own Omarchy wallpaper from
   /assets/omarchy/bg/. Pages outside this prototype open on the live site. */
(function () {
  'use strict';
  var K = window.TD_KIOSK;
  var LIVE = 'https://docs.terminal-delight.brownfamilysports.com';
  var KIOSK = 'https://terminal-delight.brownfamilysports.com';
  var WALLS = '../../assets/omarchy/bg/';
  var HERE = { '/': 'index.html', '/install': 'install.html' };
  var link = function (slug) { var p = slug.split('#'); return HERE[p[0]] ? HERE[p[0]] + (p[1] && p[0] !== '/' ? '#' + p[1] : '') : LIVE + slug; };
  var href = function (s) { return /^https?:/.test(s) ? s : link(s); };

  /* One line drawing per theme, stroked in the current ink. Proposals, Parker's to replace. */
  var G = function (d) { return '<svg class="tg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + d + '</svg>'; };
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

  var CHAPTERS = [['01', 'Start', '/'], ['02', 'Agents', '/agent-wall'], ['03', 'Bench', '/workbench'],
    ['04', 'Workspace', '/left-bar'], ['05', 'Look', '/themes'], ['06', 'MCP', '/mcp']];
  var PAGES = [['01.0', 'Introduction', '/'], ['01.1', 'Install', '/install'], ['01.2', 'First launch', '/first-launch'], ['01.3', 'Keys', '/keys']];
  var REFERENCE = [['Config files', '/config-files', 'every file'], ['Environment flags', '/environment', 'TD_ variables'],
    ['Command line', '/cli', 'every verb'], ['Plugins', '/plugins', 'MCP servers'], ['lean-ctx', '/plugins', 'token savings'],
    ['Omarchy', KIOSK + '/omarchy.html', 'the desktop half'], ['Terminal core', '/install#technical-building-from-source', 'alacritty_terminal · gpui'],
    ['Version', 'https://github.com/parker-brown-family/terminal-delight/releases/tag/v0.3.0', 'v0.3.0']];

  var here = document.body.dataset.page || '/';
  var ix = PAGES.findIndex(function (p) { return p[2] === here; });
  var prev = ix > 0 ? PAGES[ix - 1] : null, next = ix < PAGES.length - 1 ? PAGES[ix + 1] : null;
  var store = { get: function (k) { try { return localStorage.getItem(k); } catch (e) { return null; } },
    set: function (k, v) { try { localStorage.setItem(k, v); } catch (e) {} } };
  var esc = function (s) { return String(s).replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); };
  var $ = function (id) { return document.getElementById(id); };

  var ICON = {
    logo: '<svg viewBox="0 0 64 64" aria-hidden="true"><rect width="64" height="64" rx="13" fill="#1a1e24"/><path d="M9 12Q32 8.5 55 12Q59 32 55 52Q32 55.5 9 52Q5 32 9 12Z" fill="#05080a"/><clipPath id="rd-bm"><path d="M9 12Q32 8.5 55 12Q59 32 55 52Q32 55.5 9 52Q5 32 9 12Z"/></clipPath><g clip-path="url(#rd-bm)"><rect x="5" y="6" width="24" height="52" fill="#39ff7a" opacity=".92"/><rect x="34" y="6" width="25" height="24" fill="#ffb84d" opacity=".92"/><rect x="34" y="34" width="25" height="24" fill="#57d6ff" opacity=".92"/></g><path d="M31.5 6Q33 32 31.5 58" stroke="#1a1e24" stroke-width="5" fill="none"/><path d="M31 32Q45 33 60 31.5" stroke="#1a1e24" stroke-width="4.5" fill="none"/></svg>',
    search: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>',
    git: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 .5a11.5 11.5 0 0 0-3.64 22.41c.58.1.79-.25.79-.56v-2c-3.2.7-3.88-1.37-3.88-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.04-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.76 2.7 1.25 3.36.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.29 1.19-3.1-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.81 1.19 1.84 1.19 3.1 0 4.42-2.7 5.39-5.26 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 12 .5Z"/></svg>',
    /* a CRT monitor: curved screen in a deep cabinet, on a foot */
    crt: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round"><rect x="2.5" y="3" width="19" height="15" rx="2.5"/><path d="M5.5 6.2Q12 5.2 18.5 6.2Q19.3 10.5 18.5 14.8Q12 15.8 5.5 14.8Q4.7 10.5 5.5 6.2Z"/><path d="M9 21h6M10.5 18v3M13.5 18v3"/><circle cx="18.6" cy="16.4" r=".45" fill="currentColor"/></svg>',
    sun: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 3v2M12 19v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M3 12h2M19 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4"/><circle cx="12" cy="12" r="4"/></svg>',
    moon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5Z"/></svg>',
    book: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H20v15H6.5A2.5 2.5 0 0 0 4 20.5z"/><path d="M4 20.5A2.5 2.5 0 0 0 6.5 23H20v-5"/></svg>',
    ref: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m8 7-5 5 5 5M16 7l5 5-5 5"/></svg>',
    lang: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.2 3 14.8 0 18M12 3c-3 3.2-3 14.8 0 18"/></svg>',
    keys: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"/></svg>',
    bars: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 7h16M4 12h16M4 17h16"/></svg>',
  };

  /* ---------------------------------------------------------------- chrome */
  function backdrop() {
    var b = document.createElement('div');
    b.className = 'backdrop'; b.setAttribute('aria-hidden', 'true');
    b.innerHTML = '<div class="layer"></div><div class="layer"></div>';
    document.body.insertBefore(b, document.body.firstChild);
  }
  var wallOn = 0;
  function wallpaper(name) {
    var layers = document.querySelectorAll('.backdrop .layer'), next = layers[1 - wallOn], cur = layers[wallOn];
    var url = WALLS + name + '.webp';
    if (cur.dataset.name === name) return;
    var img = new Image();
    img.onload = function () { next.style.backgroundImage = 'url("' + url + '")'; next.dataset.name = name; next.classList.add('on'); cur.classList.remove('on'); wallOn = 1 - wallOn; };
    img.src = url;
  }

  function header() {
    var top = $('top');
    var refMenu = REFERENCE.map(function (r) { return '<a href="' + href(r[1]) + '">' + esc(r[0]) + '<small>' + esc(r[2]) + '</small></a>'; }).join('');
    top.className = 'top';
    top.innerHTML =
      '<div class="r1">' +
        '<div style="display:flex;align-items:center;gap:10px"><button class="ib menu-btn" id="menuBtn" aria-label="Contents">' + ICON.bars + '</button>' +
        '<a class="brand" href="index.html">' + ICON.logo + '<span class="word">terminal delight<span class="cur"></span></span></a></div>' +
        '<button class="search" id="searchBtn" type="button">' + ICON.search + '<span>Search docs</span><kbd>Ctrl K</kbd></button>' +
        '<div class="tools">' +
          '<a class="ib gh" href="https://github.com/parker-brown-family/terminal-delight" title="GitHub">' + ICON.git + '</a>' +
          '<button class="ib" id="crtBtn" aria-pressed="false" title="CRT glass (c)">' + ICON.crt + '</button>' +
          '<button class="themebtn" id="themeBtn" aria-haspopup="true" aria-expanded="false" title="Theme (t)"><span id="themeGlyph"></span><span class="nm" id="themeName"></span><span class="caret">▾</span></button>' +
          '<button class="ib" id="modeBtn" aria-pressed="false" title="Paper (m)"></button>' +
          '<a class="install" href="install.html">Install</a>' +
        '</div>' +
      '</div>' +
      '<nav class="r2" aria-label="Sections">' +
        '<a class="tab" href="index.html" aria-current="page">' + ICON.book + 'Documentation</a>' +
        '<span class="tab" tabindex="0">' + ICON.ref + 'Reference ▾<span class="menu">' + refMenu + '</span></span>' +
        '<a class="tab" href="' + LIVE + '/languages">' + ICON.lang + 'Languages</a>' +
        '<a class="tab" href="' + LIVE + '/keys">' + ICON.keys + 'Keymapping</a>' +
      '</nav>';
  }

  function sidebar() {
    var ch = CHAPTERS.map(function (c, i) {
      return '<a href="' + href(c[2]) + '"' + (i === 0 ? ' class="weak" aria-current="true"' : '') + '><span class="n">' + c[0] + ' /</span>' + esc(c[1]) + '</a>';
    }).join('');
    var pg = PAGES.map(function (p) {
      return '<a href="' + href(p[2]) + '"' + (p[2] === here ? ' class="strong" aria-current="page"' : '') + '><span class="n">' + p[0] + '</span>' + esc(p[1]) + '</a>';
    }).join('');
    $('side').innerHTML = '<div class="h">Contents</div>' + ch + '<div class="h">In 01</div>' + pg;
  }

  function pager() {
    var el = $('pager');
    if (!el) return;
    el.className = 'pager';
    el.innerHTML = (prev ? '<a href="' + href(prev[2]) + '"><small>PREVIOUS</small>← <span class="n">' + prev[0] + '</span>' + esc(prev[1]) + '</a>' : '') +
      (next ? '<a class="next" href="' + href(next[2]) + '"><small>NEXT</small><span class="n">' + next[0] + '</span>' + esc(next[1]) + ' →</a>' : '');
  }

  function overlays() {
    var d = document.createElement('div');
    d.innerHTML =
      '<div class="scrim" id="scrim"></div>' +
      '<div class="pop tray" id="tray" role="menu" aria-label="Theme"><div class="th">▸ <span id="trayNow"></span><span class="k">t · ↑↓ · ↵</span></div>' +
        '<button class="wallbtn" id="wallBtn" type="button" aria-pressed="true" title="Wallpaper (w)"><span class="dot"></span>wallpaper</button>' +
        '<div id="trayList"></div><div class="tf">Shared with the kiosks. The glyphs are proposals.</div></div>' +
      '<div class="pop finder" id="finder" role="dialog" aria-label="Search"><input id="q" placeholder="Search every page" autocomplete="off"><ol id="hits"></ol></div>' +
      '<div class="pop keys" id="keys" role="dialog" aria-label="Keys"><h3>▸ TERMINAL DELIGHT DOCS · KEYS</h3><div class="cols">' +
        '<div><h4>These pages</h4><dl>' +
          '<dt>Ctrl K · /</dt><dd>Search every page</dd><dt>[ · ]</dt><dd>Previous · next page</dd>' +
          '<dt>1 · 2 · 3</dt><dd>Brief · Story · Technical</dd><dt>t</dt><dd>Theme tray</dd><dt>w</dt><dd>Wallpaper on or off</dd>' +
          '<dt>c</dt><dd>CRT glass on or off</dd><dt>m</dt><dd>Paper or glass</dd><dt>?</dt><dd>This list</dd><dt>Esc</dt><dd>Close whatever is open</dd></dl></div>' +
        '<div><h4>In Terminal Delight</h4><dl>' +
          '<dt>Ctrl+Shift+T</dt><dd>New tab</dd><dt>Alt+V · Alt+H</dt><dd>Split beside · below</dd><dt>Alt + arrows</dt><dd>Move between panes</dd>' +
          '<dt>Alt+W</dt><dd>Close pane, into the bay</dd><dt>Ctrl+Shift+Z</dt><dd>Bring back what you closed</dd><dt>Alt+K</dt><dd>Terminal ⇄ workbench</dd>' +
          '<dt>Ctrl+Shift+N</dt><dd>Which agent needs you</dd><dt>Alt+R</dt><dd>Read this pane big</dd></dl></div>' +
      '</div><div class="kf">Every chord in the app: <a href="' + LIVE + '/keys">Keymapping →</a> · Esc to close</div></div>';
    while (d.firstChild) document.body.appendChild(d.firstChild);
  }

  /* ----------------------------------------------------------------- state */
  var open = null;
  function show(id) { hide(); open = id; $(id).classList.add('on'); $('scrim').classList.add('on');
    if (id === 'tray') { $('themeBtn').setAttribute('aria-expanded', 'true'); trayHl = K.PALETTES.findIndex(function (p) { return p.name === current.name; }); drawTray(); }
    /* focus now, not a tick later: a fast Ctrl K then typing must not lose its first letters */
    if (id === 'finder') { var q = $('q'); q.value = ''; find(''); q.focus(); } }
  function hide() { if (!open) return;
    /* closing search must hand the keys back, or the hidden box keeps swallowing them */
    if (open === 'finder' && document.activeElement) document.activeElement.blur();
    $(open).classList.remove('on'); $('scrim').classList.remove('on'); $('themeBtn').setAttribute('aria-expanded', 'false'); open = null; }

  var current = K.resolve().palette;
  var trayHl = 0;
  /* Paper keeps the theme's accents and swaps its grounds and inks. The theme module
     writes its roles inline on <html>, so paper has to write inline too to win. */
  var PAPER = { bg: '#f4efe4', bg2: '#ebe4d5', bg0: '#fbf8f1', fg: '#3a3124', fg2: '#8a7960', fgb: '#1b150d', mut: '#bcae96', sel: '#e0d5c0' };
  function repaint() {
    K.paint(current);
    if (document.documentElement.dataset.paper === 'on') Object.keys(PAPER).forEach(function (k) { document.documentElement.style.setProperty('--' + k, PAPER[k]); });
  }
  function setTheme(p, keep) {
    current = p; if (keep) K.remember(p.name);
    repaint(); wallpaper(p.name);
    $('themeGlyph').innerHTML = glyph(p.name);
    $('themeName').textContent = p.name;
    $('themeBtn').title = 'Theme: ' + p.name + ' (t)';
    $('trayNow').textContent = p.name;
  }
  function drawTray() {
    $('trayList').innerHTML = K.PALETTES.map(function (p, i) {
      return '<button class="chipg' + (i === trayHl ? ' hl' : '') + '" role="menuitemradio" aria-checked="' + (p.name === current.name) + '" data-i="' + i + '">' +
        '<span class="pie" style="background:conic-gradient(' + p.acc + ' 0 25%,' + p.grn + ' 0 50%,' + p.mag + ' 0 75%,' + p.yel + ' 0)"></span>' +
        '<span>' + esc(p.name) + '</span>' + glyph(p.name) + '</button>';
    }).join('');
  }
  function pickTray(i) { setTheme(K.PALETTES[i], true); hide(); }

  function setMode(on) { document.documentElement.dataset.paper = on ? 'on' : 'off'; store.set('td-docs-paper', on ? 'on' : 'off'); repaint();
    var b = $('modeBtn'); b.innerHTML = on ? ICON.moon : ICON.sun; b.setAttribute('aria-pressed', String(on)); b.title = (on ? 'Glass' : 'Paper') + ' (m)'; }
  function setCrt(on) { document.documentElement.dataset.crt = on ? 'on' : 'off'; store.set('td-docs-crt', on ? 'on' : 'off'); $('crtBtn').setAttribute('aria-pressed', String(on)); }
  function setWall(on) { document.documentElement.dataset.wall = on ? 'on' : 'off'; store.set('td-docs-wall', on ? 'on' : 'off'); $('wallBtn').setAttribute('aria-pressed', String(on)); }

  /* ---------------------------------------------------------------- search */
  var index = null, hits = [], hl = 0;
  /* the built site's own index; opened straight from disk the browser refuses the fetch, and search says so */
  var searchDown = false;
  fetch('../dist/search.json').then(function (r) { return r.json(); }).then(function (j) { index = j; }).catch(function () { index = []; searchDown = true; });
  function find(q) {
    var list = $('hits');
    q = q.trim().toLowerCase();
    if (!q) { hits = []; list.innerHTML = '<li class="none">Type to search every page and every version.</li>'; return; }
    if (!index) { list.innerHTML = '<li class="none">Loading…</li>'; return; }
    if (searchDown) { list.innerHTML = '<li class="none">Search needs these pages served over http; opened from disk, the browser blocks the index.</li>'; return; }
    var words = q.split(/\s+/);
    hits = index.map(function (e) {
      var t = (e.t || '').toLowerCase(), s = (e.s || '').toLowerCase(), x = (e.x || '').toLowerCase(), sc = 0;
      for (var i = 0; i < words.length; i++) { var w = words[i]; if (t.indexOf(w) >= 0) sc += 6; if (s.indexOf(w) >= 0) sc += 3; if (x.indexOf(w) >= 0) sc += 1; else if (t.indexOf(w) < 0 && s.indexOf(w) < 0) return null; }
      return { e: e, sc: sc };
    }).filter(Boolean).sort(function (a, b) { return b.sc - a.sc; }).slice(0, 8);
    hl = 0;
    if (!hits.length) { list.innerHTML = '<li class="none">Nothing matches “' + esc(q) + '”.</li>'; return; }
    list.innerHTML = hits.map(function (h, i) {
      var e = h.e, x = e.x || '', at = x.toLowerCase().indexOf(words[0]);
      var snip = at < 0 ? x.slice(0, 110) : (at > 40 ? '…' : '') + x.slice(Math.max(0, at - 40), at + 90);
      return '<li class="' + (i === 0 ? 'hl' : '') + '"><a href="' + href(e.u) + '"><b>' + esc(e.t) + '</b>' + (e.s ? '<em>' + esc(e.s) + '</em>' : '') + '<small>' + esc(snip) + '</small></a></li>';
    }).join('');
  }
  function moveHit(d) { var li = document.querySelectorAll('#hits li'); if (!hits.length) return; li[hl].classList.remove('hl'); hl = (hl + d + hits.length) % hits.length; li[hl].classList.add('hl'); li[hl].scrollIntoView({ block: 'nearest' }); }

  /* --------------------------------------------------------- register tabs */
  function regs() {
    var bar = document.querySelector('.regs');
    if (!bar) return;
    var btns = [].slice.call(bar.querySelectorAll('button'));
    function sel(name, push) {
      var b = btns.find(function (x) { return x.dataset.reg === name; }); if (!b) return;
      btns.forEach(function (x) { x.setAttribute('aria-selected', String(x === b)); });
      document.querySelectorAll('[data-reg-panel]').forEach(function (p) { p.classList.toggle('on', p.dataset.regPanel === name); });
      if (push) history.replaceState(null, '', '#' + name);
    }
    btns.forEach(function (b) { b.setAttribute('role', 'tab'); b.addEventListener('click', function () { sel(b.dataset.reg, true); }); });
    /* a hash names a register, or an element inside one */
    var isReg = function (h) { return btns.some(function (b) { return b.dataset.reg === h; }); };
    var h = location.hash.slice(1), target = h && $(h), panel = target && target.closest('[data-reg-panel]');
    sel(isReg(h) ? h : panel ? panel.dataset.regPanel : btns[0].dataset.reg, false);
    if (target && !isReg(h)) setTimeout(function () { target.scrollIntoView(); }, 0);
    window.__reg = function (n) { var b = btns[n]; if (b) sel(b.dataset.reg, true); };
    window.addEventListener('hashchange', function () {
      var h2 = location.hash.slice(1), t = h2 && $(h2);
      if (isReg(h2)) { sel(h2, false); window.scrollTo(0, 0); }
      else if (t && t.closest('[data-reg-panel]')) { sel(t.closest('[data-reg-panel]').dataset.regPanel, false); t.scrollIntoView(); }
    });
  }

  /* ------------------------------------------------------------- copy code */
  function writeClip(text) {
    if (navigator.clipboard && window.isSecureContext) return navigator.clipboard.writeText(text);
    var ta = document.createElement('textarea'); ta.value = text; document.body.appendChild(ta); ta.select();
    try { document.execCommand('copy'); } catch (e) {} ta.remove(); return Promise.resolve();
  }
  function codes() {
    document.querySelectorAll('.code').forEach(function (block) {
      var lines = [].slice.call(block.querySelectorAll('.ln'));
      /* One copy per block. It pastes as one line: no prompts, no wraps, commands joined so they run in order and stop at the first failure. */
      var text = function () { return lines.map(function (l) { return l.textContent.trim(); }).join(' && '); };
      var b = document.createElement('button'); b.className = 'copy'; b.type = 'button'; b.textContent = '⎘ copy';
      b.title = lines.length > 1 ? 'Copy as one line: every command joined with &&' : 'Copy';
      b.addEventListener('click', function () {
        var throb = function () { block.classList.remove('throb'); void block.offsetWidth; block.classList.add('throb'); b.textContent = '✓ copied';
          setTimeout(function () { b.textContent = '⎘ copy'; block.classList.remove('throb'); }, 1400); };
        writeClip(text()).then(throb, throb);
      });
      block.appendChild(b);
    });
  }

  /* ------------------------------------------------------------------ keys */
  function keys() {
    document.addEventListener('keydown', function (e) {
      var typing = /INPUT|TEXTAREA/.test(document.activeElement && document.activeElement.tagName);
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); show('finder'); return; }
      if (e.key === 'Escape') { hide(); document.body.classList.remove('side-open'); return; }
      if (open === 'finder') {
        if (e.key === 'ArrowDown') { e.preventDefault(); moveHit(1); }
        else if (e.key === 'ArrowUp') { e.preventDefault(); moveHit(-1); }
        else if (e.key === 'Enter' && hits[hl]) { location.href = document.querySelectorAll('#hits a')[hl].href; }
        return;
      }
      if (open === 'tray') {
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); trayHl = (trayHl + (e.key === 'ArrowDown' ? 1 : -1) + K.PALETTES.length) % K.PALETTES.length; drawTray(); }
        else if (e.key === 'Enter') { e.preventDefault(); pickTray(trayHl); }
        else if (e.key === 'w') setWall(document.documentElement.dataset.wall === 'off');
        return;
      }
      if (typing || e.ctrlKey || e.metaKey || e.altKey) return;
      var k = e.key;
      if (k === '/') { e.preventDefault(); show('finder'); }
      else if (k === '?') { open === 'keys' ? hide() : show('keys'); }
      else if (k === 't') show('tray');
      else if (k === 'w') setWall(document.documentElement.dataset.wall === 'off');
      else if (k === 'c') setCrt(document.documentElement.dataset.crt !== 'on');
      else if (k === 'm') setMode(document.documentElement.dataset.paper !== 'on');
      else if (k === '[' && prev) location.href = href(prev[2]);
      else if (k === ']' && next) location.href = href(next[2]);
      else if (/^[123]$/.test(k) && window.__reg) window.__reg(+k - 1);
    });
  }

  /* ------------------------------------------------------------------ boot */
  backdrop(); header(); sidebar(); pager(); overlays(); regs(); codes(); keys();
  setMode(store.get('td-docs-paper') === 'on');
  setTheme(current, false);
  setCrt(store.get('td-docs-crt') === 'on');
  setWall(store.get('td-docs-wall') !== 'off');
  $('themeBtn').addEventListener('click', function () { open === 'tray' ? hide() : show('tray'); });
  $('trayList').addEventListener('click', function (e) { var b = e.target.closest('button[data-i]'); if (b) pickTray(+b.dataset.i); });
  $('wallBtn').addEventListener('click', function () { setWall(document.documentElement.dataset.wall === 'off'); });
  $('searchBtn').addEventListener('click', function () { show('finder'); });
  $('q').addEventListener('input', function (e) { find(e.target.value); });
  $('scrim').addEventListener('click', hide);
  $('crtBtn').addEventListener('click', function () { setCrt(document.documentElement.dataset.crt !== 'on'); });
  $('modeBtn').addEventListener('click', function () { setMode(document.documentElement.dataset.paper !== 'on'); });
  $('menuBtn').addEventListener('click', function (e) { e.stopPropagation(); document.body.classList.toggle('side-open'); });
  document.querySelector('main.page').addEventListener('click', function () { document.body.classList.remove('side-open'); });
})();
