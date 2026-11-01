/* fancy-pants-brief — the rail of gel buttons that jumps between a page's parts.
   An empty <nav class="sections"> is filled from the page's h2s; a page with three or more
   h2s and no rail gets one, placed after its masthead. A rail someone wrote by hand is left
   alone. Each h2 without an id is given one from its words. */
(function () {
  var heads = [].slice.call(document.querySelectorAll('h2')).filter(function (h) {
    return !h.closest('dialog, nav, figure, .glass');
  });
  var nav = document.querySelector('nav.sections');
  if (nav && nav.children.length) return;
  if (!nav) {
    if (heads.length < 3) return;
    nav = document.createElement('nav');
    nav.className = 'sections';
    var top = document.querySelector('header.top, body > header, .wrap > header, h1');
    var after = top && (top.tagName === 'H1' ? top.parentNode : top);
    if (after && after.parentNode) after.parentNode.insertBefore(nav, after.nextSibling);
    else document.body.insertBefore(nav, document.body.firstChild);
  }
  nav.setAttribute('aria-label', 'Sections');
  var used = {};
  heads.forEach(function (h) {
    var n = h.querySelector('.n');
    if (!h.id) {
      /* "part-07-decide", never "07decide": the number and the words are kept
         apart, and an id that starts with a digit cannot be named as #07decide. */
      var text = (n ? n.textContent + ' ' : '') + (h.textContent || '').replace(n ? n.textContent : '', '');
      var base = 'part-' + (text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 44) || 'x');
      var id = base, i = 2;
      while (used[id] || document.getElementById(id)) id = base + '-' + i++;
      h.id = id;
    }
    used[h.id] = true;
    var a = document.createElement('a');
    a.href = '#' + h.id;
    if (n) { var s = document.createElement('span'); s.className = 'n'; s.textContent = n.textContent.trim(); a.appendChild(s); }
    var words = (h.textContent || '').replace(n ? n.textContent : '', '').trim();
    a.appendChild(document.createTextNode(words.length > 40 ? words.slice(0, 38) + '…' : words));
    nav.appendChild(a);
  });
})();
