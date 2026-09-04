/* ==========================================================================
   The kiosk family's shared theme spine.

   Before this file the palette table lived inside omarchy.html, which meant
   the Omarchy kiosk could wear the desktop's colours and the other six pages
   could not — and that a reader who picked a theme lost it the moment they
   followed a link. The table now lives here, once, and the pick is remembered
   under one key for the whole family, so a choice made on any kiosk survives
   the walk to every other one.

   What this module does NOT do is impose a look. It sets role variables and
   gets out of the way. A page that is built on those roles repaints whole
   (info, agents, omarchy); a page that is a physical object — the walnut TV
   cabinet, the GAMBA machine — keeps its cabinet and only wears the theme on
   the shared furniture, because a console television that turns tokyo-night
   is no longer a console television. Colour follows the page's own fiction.

   Load it before any inline script that reads TD_KIOSK, and it needs no
   `defer`: a classic script in <head> has run by the time the body's inline
   scripts do.
   ========================================================================== */
(function (global) {
  'use strict';

  /* Every palette is a real Omarchy `colors.toml` reduced to the roles a page
     can paint with, plus Last Voyage, which is ours. Keep this array in the
     order the Omarchy kiosk expects: index 0 is what an unthemed page opens
     wearing, and the rail renders in array order. */
  var PALETTES = [
  {"name":"last-voyage","mode":"dark","bg":"#14100C","bg2":"#201913","bg0":"#0B0907","fg":"#E2C79A","fg2":"#A07A50","fgb":"#F3D69B","acc":"#DD6D16","mut":"#6C4727","sel":"#54351E","red":"#E2542A","grn":"#93663B","yel":"#D8A55E","blu":"#A1561C","mag":"#B7935E","cyn":"#F58E28"},
  {"name":"gruvbox","mode":"dark","bg":"#282828","bg2":"#3c3836","bg0":"#161616","fg":"#d4be98","fg2":"#7c6f64","fgb":"#d4be98","acc":"#7daea3","mut":"#665c54","sel":"#504945","red":"#ea6962","grn":"#a9b665","yel":"#d8a657","blu":"#7daea3","mag":"#d3869b","cyn":"#89b482"},
  {"name":"osaka-jade","mode":"dark","bg":"#111c18","bg2":"#23372B","bg0":"#090f0d","fg":"#C1C497","fg2":"#81B8A8","fgb":"#F7E8B2","acc":"#509475","mut":"#53685B","sel":"#32473B","red":"#FF5345","grn":"#549e6a","yel":"#459451","blu":"#509475","mag":"#D2689C","cyn":"#2DD5B7"},
  {"name":"tokyo-night","mode":"dark","bg":"#1a1b26","bg2":"#24283b","bg0":"#0e0e14","fg":"#a9b1d6","fg2":"#565f89","fgb":"#c0caf5","acc":"#7aa2f7","mut":"#414868","sel":"#292e42","red":"#f7768e","grn":"#9ece6a","yel":"#e0af68","blu":"#7aa2f7","mag":"#ad8ee6","cyn":"#449dab"},
  {"name":"ethereal","mode":"dark","bg":"#060B1E","bg2":"#131a3a","bg0":"#030610","fg":"#ffcead","fg2":"#6d7db6","fgb":"#ffcead","acc":"#7d82d9","mut":"#6d7db6","sel":"#252e56","red":"#ED5B5A","grn":"#92a593","yel":"#E9BB4F","blu":"#7d82d9","mag":"#c89dc1","cyn":"#a3bfd1"},
  {"name":"everforest","mode":"dark","bg":"#2d353b","bg2":"#343f44","bg0":"#181d20","fg":"#d3c6aa","fg2":"#4f585e","fgb":"#d3c6aa","acc":"#7fbbb3","mut":"#475258","sel":"#3d484d","red":"#e67e80","grn":"#a7c080","yel":"#dbbc7f","blu":"#7fbbb3","mag":"#d699b6","cyn":"#83c092"},
  {"name":"miasma","mode":"dark","bg":"#222222","bg2":"#2c2c2c","bg0":"#121212","fg":"#c2c2b0","fg2":"#555555","fgb":"#c2c2b0","acc":"#78824b","mut":"#666666","sel":"#383838","red":"#685742","grn":"#5f875f","yel":"#b36d43","blu":"#78824b","mag":"#bb7744","cyn":"#c9a554"},
  {"name":"retro-82","mode":"dark","bg":"#05182e","bg2":"#0a2540","bg0":"#020c17","fg":"#f6dcac","fg2":"#3f8f8a","fgb":"#f6dcac","acc":"#faa968","mut":"#2a6b78","sel":"#134e5a","red":"#f85525","grn":"#028391","yel":"#e97b3c","blu":"#3f8f8a","mag":"#3f8f8a","cyn":"#8cbfb8"}
  ];

  /* Terminal slot ← role. The Omarchy kiosk renders this as a table to make
     the mapping legible; it lives here because it is a property of the
     palette format, not of that one page. */
  var ROLE_SLOTS = [
    ['bg',  'black',    'background'],
    ['red', 'red',      'red'],
    ['grn', 'green',    'green'],
    ['yel', 'yellow',   'yellow'],
    ['blu', 'blue',     'blue'],
    ['mag', 'magenta',  'magenta'],
    ['cyn', 'cyan',     'cyan'],
    ['fg',  'white',    'foreground'],
    ['mut', 'br black', 'muted'],
    ['fgb', 'cursor',   'bright_fg'],
  ];

  var VARS = ['bg','bg2','bg0','fg','fg2','fgb','acc','mut','sel','red','grn','yel','blu','mag','cyn'];

  /* One key for the family. The Omarchy kiosk shipped first and wrote its own
     key, so a reader who chose a theme before this existed still has one — it
     is migrated forward on first read rather than discarded, because losing a
     deliberate choice to a refactor is exactly the kind of small rudeness
     nobody ever files a bug about. */
  var KEY = 'td-kiosk-theme';
  var LEGACY_KEY = 'td-omarchy-theme';

  function byName(name) {
    if (!name) return null;
    for (var i = 0; i < PALETTES.length; i++) if (PALETTES[i].name === name) return PALETTES[i];
    return null;
  }

  function recall() {
    try {
      var v = global.localStorage.getItem(KEY);
      if (v) return v;
      var old = global.localStorage.getItem(LEGACY_KEY);
      if (old) { global.localStorage.setItem(KEY, old); return old; }
    } catch (e) { /* private window, or site data blocked */ }
    return null;
  }

  function remember(name) {
    try { global.localStorage.setItem(KEY, name); } catch (e) { /* as above */ }
  }

  function fromQuery() {
    try {
      return new URLSearchParams(global.location.search).get('t');
    } catch (e) { return null; }
  }

  /* Writes the role variables and the two attributes a stylesheet can hang
     off. Deliberately does nothing else: each page decides what a repaint
     means for its own furniture. */
  function paint(palette, root) {
    var el = root || document.documentElement;
    for (var i = 0; i < VARS.length; i++) el.style.setProperty('--' + VARS[i], palette[VARS[i]]);
    el.setAttribute('data-theme', palette.name);
    if (document.body) document.body.setAttribute('data-mode', palette.mode === 'light' ? 'light' : 'dark');
    return palette;
  }

  /* Resolve which palette this page load should open wearing.
     `?t=` beats a remembered pick beats the default — a link someone was
     handed should show what the sender saw. Both arrive PINNED, which the
     Omarchy kiosk's scroll spine reads to know it must not take the wheel. */
  function resolve() {
    var q = byName(fromQuery());
    if (q) return { palette: q, pinned: true, source: 'query' };
    var r = byName(recall());
    if (r) return { palette: r, pinned: true, source: 'stored' };
    return { palette: PALETTES[0], pinned: false, source: 'default' };
  }

  /* A compact picker for the pages that repaint. The Omarchy kiosk builds its
     own — the chips there scatter and reassemble on scroll, which is that
     page's whole argument — so this is only for everyone else. Returns the
     element so a caller can place it. */
  function mountRail(host, opts) {
    if (!host) return null;
    var o = opts || {};
    var onPick = o.onPick || function () {};
    host.classList.add('kiosk-rail');
    host.setAttribute('role', 'group');
    host.setAttribute('aria-label', 'page theme');

    var lab = document.createElement('span');
    lab.className = 'kiosk-rail-lab';
    lab.textContent = o.label || 'theme';
    host.appendChild(lab);

    PALETTES.forEach(function (p) {
      var b = document.createElement('button');
      b.type = 'button';
      b.className = 'kiosk-chip';
      b.dataset.name = p.name;
      b.title = p.name + ' · ' + p.mode;
      b.setAttribute('aria-pressed', 'false');
      /* The swatch carries four of the palette's roles, not one, because a
         single accent square makes gruvbox and everforest look identical at
         14px — and those are exactly the two a reader is choosing between. */
      var sw = document.createElement('span');
      sw.className = 'kiosk-sw';
      sw.style.setProperty('--c1', p.acc);
      sw.style.setProperty('--c2', p.grn);
      sw.style.setProperty('--c3', p.mag);
      sw.style.setProperty('--c4', p.yel);
      var nm = document.createElement('span');
      nm.className = 'kiosk-nm';
      nm.textContent = p.name;
      b.appendChild(sw);
      b.appendChild(nm);
      b.addEventListener('click', function () {
        paint(p);
        remember(p.name);
        mark(host, p.name);
        onPick(p);
      });
      host.appendChild(b);
    });
    return host;
  }

  function mark(host, name) {
    var chips = (host || document).querySelectorAll('.kiosk-chip');
    for (var i = 0; i < chips.length; i++)
      chips[i].setAttribute('aria-pressed', String(chips[i].dataset.name === name));
  }

  global.TD_KIOSK = {
    PALETTES: PALETTES,
    ROLE_SLOTS: ROLE_SLOTS,
    VARS: VARS,
    byName: byName,
    paint: paint,
    resolve: resolve,
    recall: recall,
    remember: remember,
    fromQuery: fromQuery,
    mountRail: mountRail,
    mark: mark,
  };
})(window);
