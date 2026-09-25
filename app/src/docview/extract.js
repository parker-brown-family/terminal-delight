/* Terminal Delight's page probe: run inside the brief, after it has loaded, by
 * the snapshot engine (docview/snapshot.rs). It never changes the page's
 * layout and never touches the file; it reports where things are.
 *
 * Everything here reads the page the brief's OWN notes.js produced. The
 * corpus carries ten different copies of notes.js (three releases, seven
 * hand-edited forks), so TD never re-implements the tagging: it asks the page
 * which elements were tagged, and with what ids.
 *
 * Rects are CSS px in page coordinates (scroll added back), except inside a
 * dialog render, where they are relative to the dialog's own box. A 0 x 0
 * rect is reported as it is and read on the Rust side as "not laid out"
 * (inside a closed dialog, or display: none), never as a place.
 *
 * Installed once per page; calling it again is harmless. Evaluated with
 * returnByValue, so every answer is plain JSON.
 */
(function () {
  'use strict';
  if (window.__terminalDelight) return;

  /* The page's own notes chrome is drawn into the picture otherwise: the fixed
     notebar lands over body text in the first viewport, and the note-count
     badges go stale the moment TD adds a note. Hidden with visibility, which
     keeps every box where it was: the spike measured that this moved no anchor
     in any of 119 files.
     The rule notes.css draws down the left edge of an anchor with notes goes
     too. A box-shadow takes no space, so nothing moves; and a picture that
     carried it would be a picture of the notes as they were when it was taken,
     wrong the moment a note is added or deleted, while TD draws its own rule
     from the file (docview/notes_ui.rs). The picture shows the page; the notes
     are drawn over it. */
  var HIDE = '.notebar, .note-btn, .concur-zone { visibility: hidden !important; }\n' +
    '.notable.has-note, tr.notable.has-note > td:first-child { box-shadow: none !important; }';

  function hideNotesUi() {
    if (document.getElementById('td-hide-notes-ui')) return;
    var s = document.createElement('style');
    s.id = 'td-hide-notes-ui';
    s.textContent = HIDE;
    (document.head || document.documentElement).appendChild(s);
  }

  function box(el, dx, dy) {
    if (!el) return null;
    var r = el.getBoundingClientRect();
    return { x: r.x + dx, y: r.y + dy, w: r.width, h: r.height };
  }

  function dialogOf(el) {
    var d = el.closest('dialog');
    return d ? (d.id || null) : null;
  }

  /* HOOK for the notes layer: every element the brief's notes.js tagged, in
     document order, with the page's own (hidden) note button and concur zone,
     so TD can draw its buttons exactly where a browser draws them. */
  function anchorsIn(root, dx, dy) {
    return Array.prototype.map.call(root.querySelectorAll('.notable'), function (el) {
      return {
        nid: el.dataset.nid || '',
        title: el.dataset.ntitle || '',
        tag: el.tagName.toLowerCase(),
        dialog: dialogOf(el),
        rect: box(el, dx, dy),
        button: box(el.querySelector(':scope > .note-btn'), dx, dy),
        concur_zone: box(el.querySelector(':scope > .concur-zone'), dx, dy),
        /* What the page's own notes.js shows on it: a save reads the written
           file back and checks these against what it wrote. */
        has_note: el.classList.contains('has-note'),
        has_concur: el.classList.contains('has-concur')
      };
    });
  }

  function here() {
    return location.href.split('#')[0];
  }

  /* The element a fragment names, found the way a browser finds it: the
     first element with that id, else the first <a> with that name. */
  function named(id) {
    var t = document.getElementById(id);
    if (t) return t;
    var byName = document.getElementsByName(id);
    for (var i = 0; i < byName.length; i++) {
      if (byName[i].tagName === 'A') return byName[i];
    }
    return null;
  }

  /* Where the element a fragment names starts, or null when it names
     nothing that is laid out. A fragment of "top" naming nothing is the top
     of the page, as it is in a browser. */
  function topOf(id, dy) {
    var t = named(id);
    if (!t) return id.toLowerCase() === 'top' ? 0 : null;
    var r = t.getBoundingClientRect();
    if (!r.width && !r.height) return null;
    return r.y + dy;
  }

  /* Where a same-page fragment's target starts, or null when it names nothing. */
  function fragmentTop(a, dy) {
    var url = a.href;
    var hash = url.indexOf('#');
    if (hash < 0 || url.slice(0, hash) !== here()) return null;
    var id = url.slice(hash + 1);
    try { id = decodeURIComponent(id); } catch (e) { /* keep it as written */ }
    if (!id) return 0;
    return topOf(id, dy);
  }

  /* Every name a fragment arriving from another document can use, each id
     and each <a name>, once, with where the element it picks starts: the
     same lookup a link on the page itself gets. */
  function targetsIn(dy) {
    var seen = Object.create(null);
    var out = [];
    Array.prototype.forEach.call(document.querySelectorAll('[id], a[name]'), function (el) {
      [el.id, el.tagName === 'A' ? el.getAttribute('name') : ''].forEach(function (id) {
        if (!id || seen[id]) return;
        seen[id] = true;
        out.push({ id: id, top: topOf(id, dy) });
      });
    });
    return out;
  }

  function linksIn(root, dx, dy) {
    return Array.prototype.map.call(root.querySelectorAll('a[href]'), function (a) {
      return {
        href: a.getAttribute('href'),
        resolved: a.href,
        rect: box(a, dx, dy),
        dialog: dialogOf(a),
        fragment_top_css: fragmentTop(a, window.scrollY)
      };
    });
  }

  function openersIn(root, dx, dy) {
    return Array.prototype.map.call(root.querySelectorAll('[data-dlg]'), function (b) {
      return { dialog: b.dataset.dlg, rect: box(b, dx, dy), inside: dialogOf(b) };
    });
  }

  function contentDialogs() {
    return Array.prototype.filter.call(document.querySelectorAll('dialog'), function (d) {
      return d.id !== 'd-note' && d.id !== 'd-export';
    });
  }

  /* Whether the brief's own notes.js takes concurs: only a script that makes
     concur zones says so. A .concur-zone in the markup does not — one
     brief in the archive, a sticker study from before the concur release,
     draws its own, and its notes.js never reads a concurs island, so a
     stamp written there would never show (the read-back after a save found
     it). */
  function concurSupport(tagged) {
    var scripts = document.querySelectorAll('script:not([src])');
    for (var i = 0; i < scripts.length; i++) {
      if (scripts[i].textContent.indexOf('function concurZones') >= 0) return 'Supported';
    }
    return tagged ? 'NotSupported' : 'Unknown';
  }

  /* How far the most-overflowing scroller inside a dialog would scroll. */
  function overflow(d) {
    var need = 0;
    [d].concat(Array.prototype.slice.call(d.querySelectorAll('*'))).forEach(function (el) {
      var o = getComputedStyle(el).overflowY;
      if (o === 'visible') return;
      var extra = el.scrollHeight - el.clientHeight;
      if (extra > 1 && extra > need) need = extra;
    });
    return need;
  }

  window.__terminalDelight = {
    extract: function () {
      hideNotesUi();
      window.scrollTo(0, 0);
      var dx = window.scrollX, dy = window.scrollY;
      var tagged = document.querySelectorAll('.notable').length;
      var diagnostics = (window.__tdErrors || []).map(function (m) { return 'page error: ' + m; });
      var dialogs = [];
      contentDialogs().forEach(function (d) {
        if (d.id) dialogs.push(d.id);
        else diagnostics.push('a dialog without an id cannot be opened');
      });
      var openers = openersIn(document, dx, dy);
      openers.forEach(function (o) {
        if (!document.getElementById(o.dialog)) diagnostics.push('opener for a missing dialog: ' + o.dialog);
      });
      var file = null;
      if (tagged) file = window.NOTES_FILE || (location.pathname.split('/').pop() || 'decision-brief.html');
      return {
        height_css: Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0),
        notes_file: file,
        capability: {
          notes_islands: document.querySelectorAll('[id="report-notes"]').length,
          concurs_island: !!document.getElementById('report-concurs'),
          tagged: tagged,
          concur: concurSupport(tagged)
        },
        anchors: anchorsIn(document, dx, dy),
        links: linksIn(document, dx, dy).filter(function (l) { return !l.dialog; }),
        targets: targetsIn(dy),
        openers: openers.filter(function (o) { return !o.inside; }),
        dialogs: dialogs,
        diagnostics: diagnostics
      };
    },

    /* Open a dialog by id, the way its button would, and say how far its
       body would still scroll at this viewport. */
    dialogOpen: function (id) {
      hideNotesUi();
      window.scrollTo(0, 0);
      var d = document.getElementById(id);
      if (!d || d.tagName !== 'DIALOG') return { ok: false, overflow: 0 };
      if (!d.open) {
        try { d.showModal(); } catch (e) { return { ok: false, overflow: 0, error: String(e) }; }
      }
      return { ok: d.open, overflow: overflow(d) };
    },

    /* The open dialog's box, in viewport coordinates (the page is at the
       top), and everything inside it relative to that box. */
    dialogMeasure: function (id) {
      var d = document.getElementById(id);
      if (!d || !d.open) return null;
      var r = d.getBoundingClientRect();
      var dx = -r.x, dy = -r.y;
      return {
        rect: { x: r.x, y: r.y, w: r.width, h: r.height },
        overflow: overflow(d),
        anchors: anchorsIn(d, dx, dy),
        links: linksIn(d, dx, dy),
        openers: openersIn(d, dx, dy),
        closers: Array.prototype.map.call(d.querySelectorAll('[data-close]'), function (b) {
          return box(b, dx, dy);
        })
      };
    },

    dialogClose: function (id) {
      var d = document.getElementById(id);
      if (d && d.open) d.close();
      return true;
    }
  };
})();
