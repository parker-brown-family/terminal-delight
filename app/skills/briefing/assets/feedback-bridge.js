/* Briefing handoff bridge. The shared notes engine and its islands remain canonical.
   Show and export orphan comments rather than letting a missing DOM target hide them. */
(function () {
  'use strict';
  function snapshot() {
    var detail = {};
    document.dispatchEvent(new CustomEvent('decision-brief:snapshot', { detail: detail }));
    if (!detail.html) return null;
    var saved = new DOMParser().parseFromString(detail.html, 'text/html');
    return {
      notes: JSON.parse(saved.getElementById('report-notes').textContent),
      concurs: JSON.parse(saved.getElementById('report-concurs').textContent),
      meta: JSON.parse(saved.getElementById('brief-meta').textContent)
    };
  }
  function missing(data) {
    var live = new Set(Array.from(document.querySelectorAll('[data-nid]')).map(function (e) { return e.dataset.nid; }));
    return Array.from(new Set(Object.keys(data.notes).concat(Object.keys(data.concurs)))).filter(function (id) { return !live.has(id); });
  }
  function showStatus(data) {
    var prior = document.getElementById('brief-feedback-status');
    if (prior) prior.remove();
    var orphaned = missing(data);
    var hasFeedback = Object.keys(data.notes).length || Object.keys(data.concurs).length;
    if (orphaned.length || (hasFeedback && data.meta.feedback_revision === null)) {
      var status = document.createElement('aside');
      status.id = 'brief-feedback-status';
      status.className = 'callout';
      status.setAttribute('role', 'status');
      var title = document.createElement('strong');
      title.textContent = 'Earlier feedback needs review';
      var text = document.createElement('p');
      text.textContent = 'Some comments refer to earlier wording or a removed target. Their text is retained; review the prior brief before acting. Copy map includes removed targets.';
      status.append(title, text);
      orphaned.forEach(function (id) {
        var line = document.createElement('p');
        line.textContent = '[' + id + '] Unresolved target: ' + (data.notes[id] || []).map(function (n) { return n.text; }).join(' · ') + (data.concurs[id] ? ' · explicit agreement on the earlier version' : '');
        status.appendChild(line);
      });
      document.body.appendChild(status);
    }
    var total = Object.values(data.notes).reduce(function (n, entries) { return n + entries.length; }, 0);
    document.getElementById('note-count').textContent = total;
    document.getElementById('btn-export').disabled = total + Object.keys(data.concurs).length === 0;
    document.getElementById('btn-clear').disabled = total === 0;
  }
  function init() {
    var data = snapshot();
    if (!data) return;
    showStatus(data);
    document.addEventListener('click', function (event) {
      if (!event.target.closest('[data-note-action], .concur-zone, .ndel')) return;
      var current = snapshot();
      if (!current) return;
      showStatus(current);
      if (!event.target.closest('#btn-export')) return;
      var field = document.getElementById('export-out');
      var extra = current.meta.feedback_revision === null ? '\nREVISION UNKNOWN — review the prior brief before acting.\n' : '';
      missing(current).forEach(function (id) {
        extra += '\n[' + id + '] UNRESOLVED — target missing\n';
        (current.notes[id] || []).forEach(function (n) { extra += '  · ' + n.text.replace(/\n+/g, ' ') + '\n'; });
        if (current.concurs[id]) extra += '  · Explicit agreement on the earlier target\n';
      });
      field.value += extra;
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
