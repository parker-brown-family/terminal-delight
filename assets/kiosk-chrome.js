/* ==========================================================================
   Boots the shared Omarchy theme on the kiosks that wear it.

   This file used to build the family strip as well: a bar across the top of
   every kiosk naming all the others. It is gone as of 2026-09-24. On the
   Omarchy kiosk it sat above the page's own navigation, so the page opened
   under two top bars, and Parker's call on seeing it was a flat no. The
   kiosks link out through their own navigation and on-screen buttons, and the
   info page and the docs carry the way between them.

   What stays is the theme boot, because a palette picked on one kiosk still
   has to be worn by the next. Usage, in <head>, after kiosk-theme.js:

     <script src="/assets/kiosk-chrome.js" data-kiosk="tv" data-paint="strip"></script>

   `data-paint` decides where the role variables land. "root" repaints the
   whole document, for a page built out of roles. "none" leaves painting to
   the page, as the Omarchy kiosk's own scroll spine does. "strip" was for the
   cabinets, whose theme lived on the strip alone so the walnut stayed walnut
   and GAMBA kept its own `--red`; with no strip there is nothing to paint, so
   it now does nothing, which is still the right thing for a cabinet.
   `data-kiosk`, `data-rail` and `data-float` are accepted and ignored, so no
   page has to change.
   ========================================================================== */
(function () {
  'use strict';

  var self = document.currentScript;
  var paintRoot = ((self && self.dataset.paint) || 'root') === 'root';
  if (!paintRoot || !window.TD_KIOSK) return;

  /* Resolved at script time, so the page is already wearing its theme before
     the first frame rather than flashing the default and correcting itself. */
  var chosen = window.TD_KIOSK.resolve().palette;
  window.TD_KIOSK.paint(chosen);

  /* That paint ran in <head>, where there was no <body> yet to carry the
     light/dark flag. Running it again once the body exists costs nothing. */
  var again = function () { window.TD_KIOSK.paint(chosen); };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', again);
  else again();
})();
