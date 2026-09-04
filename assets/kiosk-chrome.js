/* ==========================================================================
   Builds the family strip, and boots the shared theme.

   The strip is generated rather than pasted into eight files on purpose: it
   names every kiosk, so every time one is added or renamed a hand-maintained
   copy in seven other pages would go stale, and the stale ones would be the
   pages nobody opens — which is exactly where a dead link survives longest.

   It is additive. No page's existing links are replaced by it, so a reader
   with scripting off is never worse off than before this file existed.

   Usage, in <head>, after kiosk-theme.js:

     <script src="assets/kiosk-chrome.js" data-kiosk="tv"></script>

   `data-kiosk` names which member of the family this page is, and that entry
   renders as the current one rather than as a link. `data-rail="off"` omits
   the theme picker for pages that do not repaint — the cabinets — where a
   picker that changed only the strip would be a control that lies.

   `data-paint` decides WHERE the role variables land, and it is the setting
   that keeps this from vandalising the devices. On a page built out of roles
   it is "root", and the theme repaints the whole document. On a cabinet it is
   "strip", and the roles are written onto the strip element only — so the
   strip wears the theme while the walnut stays walnut and, more to the point,
   while GAMBA keeps its own `--red`, which a root-level repaint would
   silently overwrite with whichever red the palette happened to carry.
   ========================================================================== */
(function () {
  'use strict';

  var self = document.currentScript;
  var here = (self && self.dataset.kiosk) || '';
  var wantRail = !(self && self.dataset.rail === 'off');
  var floating = !!(self && self.dataset.float === 'on');
  /* "root" (default) · "strip" · "none". "none" is for the Omarchy kiosk,
     whose own scroll spine owns the paint and would fight a second writer. */
  var paintMode = (self && self.dataset.paint) || 'root';
  var paintRoot = paintMode === 'root';

  /* Order is the tour, not the alphabet: the two pages that explain the thing
     first, then the three devices, then the two set-pieces. */
  var FAMILY = [
    { id: 'info',   href: '/info.html',        label: 'info' },
    { id: 'omarchy',href: '/omarchy.html',     label: 'omarchy' },
    { id: 'agents', href: '/agents.html',      label: 'agents' },
    { id: 'tv',     href: '/tv.html',          label: 'tv' },
    { id: 'global', href: '/global.html',      label: 'global' },
    { id: 'gamba',  href: '/gamba.html',       label: 'gamba' },
    { id: 'crawl',  href: '/start-crawl.html', label: 'crawl' },
  ];

  function build() {
    if (document.querySelector('.kiosk-strip')) return;

    var strip = document.createElement('nav');
    strip.className = 'kiosk-strip' + (floating ? ' float' : '');
    strip.setAttribute('aria-label', 'terminal-delight kiosks');

    var home = document.createElement('a');
    home.className = 'k-home';
    home.href = '/info.html';
    var mark = document.createElement('img');
    mark.src = '/favicon.svg';
    mark.alt = '';
    mark.width = 15; mark.height = 15;
    home.appendChild(mark);
    var wordmark = document.createElement('span');
    wordmark.textContent = 'terminal-delight';
    home.appendChild(wordmark);
    strip.appendChild(home);

    var sep = document.createElement('span');
    sep.className = 'k-sep';
    strip.appendChild(sep);

    FAMILY.forEach(function (k) {
      if (k.id === here) {
        var cur = document.createElement('span');
        cur.className = 'k-here';
        cur.textContent = k.label;
        cur.setAttribute('aria-current', 'page');
        strip.appendChild(cur);
      } else {
        var a = document.createElement('a');
        a.className = 'k-link';
        a.href = k.href;
        a.textContent = k.label;
        strip.appendChild(a);
      }
    });

    var spacer = document.createElement('span');
    spacer.className = 'k-spacer';
    strip.appendChild(spacer);

    if (wantRail && window.TD_KIOSK) {
      var rail = document.createElement('span');
      strip.appendChild(rail);
      window.TD_KIOSK.mountRail(rail, {
        onPick: function (p) {
          if (!paintRoot) window.TD_KIOSK.paint(p, strip);
          /* Let the host page react — the agent wall re-tints its group
             colours, the Omarchy kiosk re-weights its scrim. */
          window.dispatchEvent(new CustomEvent('kiosk:theme', { detail: p }));
        },
      });
    }

    document.body.insertBefore(strip, document.body.firstChild);

    if (!window.TD_KIOSK || !chosen) return;
    /* A cabinet's roles land here and nowhere else. Done after insertion so
       the element is live when the variables are written to it. */
    if (paintMode === 'strip') window.TD_KIOSK.paint(chosen, strip);
    /* The early paint ran in <head>, where there was no <body> yet to carry
       the light/dark flag. Re-running it now costs nothing and sets it. */
    if (paintRoot) window.TD_KIOSK.paint(chosen);
    window.TD_KIOSK.mark(strip, chosen.name);
  }

  /* Resolved once, at script time, so a root-painted page is already wearing
     its theme before the first frame rather than flashing the default and
     correcting itself. */
  var chosen = null;
  if (window.TD_KIOSK) {
    chosen = window.TD_KIOSK.resolve().palette;
    if (paintRoot) window.TD_KIOSK.paint(chosen);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', build);
  } else {
    build();
  }
})();
