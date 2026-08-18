/* ============================================================
   theme-engine.js — seed-driven palette + theme selector
   Ported from the IMT PM theme engine (battle-tested), adapted for
   terminal-delight: hacker default, td_ storage keys, BroadcastChannel
   sync so detached popout windows stay in lockstep with the main window.

   A seed hex -> derivePalette() -> --theme-* CSS vars (HSL math).
   theme.css maps those to semantic tokens per [data-theme].
   ============================================================ */
export const THEME_CHANNEL = 'td-theme';

const STORAGE_KEY = 'td_theme';
const ACCENT_KEY = 'td_theme_accent';
const SCALE_KEY = 'td_ui_scale';
const DEFAULT_THEME = 'hacker';
const DEFAULT_ACCENTS = {
  'quiet-command': '#2f6fdd',
  'field-command': '#8fa85f',
  'tactical-overdrive': '#31d7ff',
  hacker: '#22c55e',
};
const THEMES = new Set(Object.keys(DEFAULT_ACCENTS));
const DARK = new Set(['field-command', 'tactical-overdrive', 'hacker']);
const MIN_SCALE = 0.9, MAX_SCALE = 1.35, DEFAULT_SCALE = 1;
const DEFAULT_ACCENT = DEFAULT_ACCENTS[DEFAULT_THEME];

let channel = null;
try { channel = new BroadcastChannel(THEME_CHANNEL); } catch { /* unsupported */ }

/* ---------- storage (best-effort) ---------- */
const get = (k, f) => { try { return localStorage.getItem(k) ?? f; } catch { return f; } };
const raw = (k) => { try { return localStorage.getItem(k); } catch { return null; } };
const set = (k, v) => { try { localStorage.setItem(k, v); } catch {} };
const del = (k) => { try { localStorage.removeItem(k); } catch {} };

const clampScale = (v) => { const n = Number(v); return Number.isFinite(n) ? Math.min(MAX_SCALE, Math.max(MIN_SCALE, n)) : DEFAULT_SCALE; };

function normalizeHex(value) {
  const r = String(value || '').trim();
  if (/^#[0-9a-fA-F]{6}$/.test(r)) return r.toLowerCase();
  if (/^#[0-9a-fA-F]{3}$/.test(r)) return ('#' + r.slice(1).split('').map(c => c + c).join('')).toLowerCase();
  return DEFAULT_ACCENT;
}

const accentKey = (theme) => `${ACCENT_KEY}:${theme || readTheme()}`;
const defaultAccent = (theme) => DEFAULT_ACCENTS[theme] || DEFAULT_ACCENT;
function readTheme() { const s = get(STORAGE_KEY, DEFAULT_THEME); return THEMES.has(s) ? s : DEFAULT_THEME; }
function readAccent(theme) { const t = theme || readTheme(); return normalizeHex(raw(accentKey(t)) || defaultAccent(t)); }
function readScale() { return clampScale(get(SCALE_KEY, DEFAULT_SCALE)); }

/* ---------- colour math ---------- */
function hexToRgb(hex) { const c = normalizeHex(hex).slice(1); return { r: parseInt(c.slice(0, 2), 16), g: parseInt(c.slice(2, 4), 16), b: parseInt(c.slice(4, 6), 16) }; }
function rgbToHsl({ r, g, b }) {
  const rn = r / 255, gn = g / 255, bn = b / 255;
  const max = Math.max(rn, gn, bn), min = Math.min(rn, gn, bn);
  let h = 0, s = 0; const l = (max + min) / 2, d = max - min;
  if (d) { s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    if (max === rn) h = ((gn - bn) / d + (gn < bn ? 6 : 0)) / 6;
    else if (max === gn) h = ((bn - rn) / d + 2) / 6; else h = ((rn - gn) / d + 4) / 6; }
  return { h: h * 360, s: s * 100, l: l * 100 };
}
function hslToRgb({ h, s, l }) {
  const hn = (((h % 360) + 360) % 360) / 360, sn = Math.max(0, Math.min(100, s)) / 100, ln = Math.max(0, Math.min(100, l)) / 100;
  if (sn === 0) { const v = Math.round(ln * 255); return { r: v, g: v, b: v }; }
  const f = (p, q, t) => { if (t < 0) t += 1; if (t > 1) t -= 1; if (t < 1 / 6) return p + (q - p) * 6 * t; if (t < 1 / 2) return q; if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6; return p; };
  const q = ln < 0.5 ? ln * (1 + sn) : ln + sn - ln * sn, p = 2 * ln - q;
  return { r: Math.round(f(p, q, hn + 1 / 3) * 255), g: Math.round(f(p, q, hn) * 255), b: Math.round(f(p, q, hn - 1 / 3) * 255) };
}
const toHex = ({ r, g, b }) => '#' + [r, g, b].map(v => Math.max(0, Math.min(255, v)).toString(16).padStart(2, '0')).join('');
const at = (base, dh, s, l) => toHex(hslToRgb({ h: base.h + dh, s: Math.max(18, Math.min(100, s)), l: Math.max(8, Math.min(92, l)) }));

function derivePalette(hex) {
  const rgb = hexToRgb(hex), hsl = rgbToHsl(rgb);
  const lum = (0.2126 * rgb.r + 0.7152 * rgb.g + 0.0722 * rgb.b) / 255;
  const sat = Math.max(18, hsl.s), light = Math.max(34, Math.min(72, hsl.l));
  const accent = normalizeHex(hex);
  const complement = at(hsl, 180, Math.max(42, sat - 8), Math.min(72, light + 4));
  const field = at(hsl, 92, Math.max(38, sat - 18), Math.min(66, light - 2));
  const warm = at(hsl, 34, Math.max(54, sat - 4), Math.min(70, light + 2));
  return {
    accent, accentRgb: rgb,
    strong: at(hsl, 0, Math.min(100, sat + 10), Math.min(84, light + 18)),
    muted: at(hsl, 0, Math.max(32, sat - 30), Math.max(42, light - 8)),
    dark: at(hsl, 0, Math.max(38, sat - 12), Math.max(16, light - 36)),
    complement, complementRgb: hexToRgb(complement),
    field, fieldRgb: hexToRgb(field),
    warm, warmRgb: hexToRgb(warm),
    ink: lum > 0.58 ? '#04140a' : '#ecfff4',
  };
}

/* ---------- apply ---------- */
const root = document.documentElement;
const setVar = (n, v) => root.style.setProperty(n, v);
const trip = ({ r, g, b }) => `${r} ${g} ${b}`;

function applyPalette(color, persist, themeOverride, broadcast = true) {
  const theme = themeOverride || readTheme();
  const accent = normalizeHex(color);
  const p = derivePalette(accent);
  setVar('--theme-accent', p.accent);
  setVar('--theme-accent-rgb', trip(p.accentRgb));
  setVar('--theme-accent-strong', p.strong);
  setVar('--theme-accent-muted', p.muted);
  setVar('--theme-accent-dark', p.dark);
  setVar('--theme-accent-ink', p.ink);
  setVar('--theme-complement', p.complement);
  setVar('--theme-complement-rgb', trip(p.complementRgb));
  setVar('--theme-field', p.field);
  setVar('--theme-field-rgb', trip(p.fieldRgb));
  setVar('--theme-warm', p.warm);
  setVar('--theme-warm-rgb', trip(p.warmRgb));
  document.querySelectorAll('[data-theme-accent]').forEach(i => { i.value = accent; });
  document.querySelectorAll('[data-palette-value]').forEach(b => {
    const active = normalizeHex(b.dataset.paletteValue) === accent;
    b.classList.toggle('is-active', active);
    b.setAttribute('aria-pressed', active ? 'true' : 'false');
  });
  if (persist) { set(accentKey(theme), accent); if (theme === DEFAULT_THEME) set(ACCENT_KEY, accent); }
  if (broadcast && channel) channel.postMessage({ kind: 'accent', theme, accent });
}

function applyTheme(theme, persist, broadcast = true) {
  const next = THEMES.has(theme) ? theme : DEFAULT_THEME;
  root.setAttribute('data-theme', next);
  root.style.colorScheme = DARK.has(next) ? 'dark' : 'light';
  applyPalette(readAccent(next), false, next, false);
  document.querySelectorAll('[data-theme-value]').forEach(b => {
    const active = b.dataset.themeValue === next;
    b.classList.toggle('is-active', active);
    b.setAttribute('aria-pressed', active ? 'true' : 'false');
  });
  document.querySelector('[data-active-theme-label]') && (document.querySelector('[data-active-theme-label]').textContent = next);
  if (persist) set(STORAGE_KEY, next);
  if (broadcast && channel) channel.postMessage({ kind: 'theme', theme: next });
}

function applyScale(scale, persist, broadcast = true) {
  const s = clampScale(scale), inv = 1 / s, label = `${Math.round(s * 100)}%`;
  root.style.setProperty('--ui-scale', String(s));
  root.style.setProperty('--ui-scale-inverse', String(inv));
  document.querySelectorAll('[data-ui-scale]').forEach(i => { i.value = String(s); i.setAttribute('aria-valuetext', label); });
  document.querySelectorAll('[data-ui-scale-value]').forEach(o => { o.textContent = label; });
  if (persist) set(SCALE_KEY, String(s));
  if (broadcast && channel) channel.postMessage({ kind: 'scale', scale: s });
}

function resetAccent() { const t = readTheme(); del(accentKey(t)); if (t === DEFAULT_THEME) del(ACCENT_KEY); applyPalette(defaultAccent(t), false, t); }

/* ---------- theme menu open/close ---------- */
function setMenuOpen(menu, open) {
  const trigger = menu.querySelector('[data-theme-menu-trigger]');
  const panel = menu.querySelector('[data-theme-menu-panel]');
  if (!trigger || !panel) return;
  panel.hidden = !open;
  menu.classList.toggle('is-open', open);
  trigger.setAttribute('aria-expanded', open ? 'true' : 'false');
}
function closeMenus(except) { document.querySelectorAll('[data-theme-menu].is-open').forEach(m => { if (m !== except) setMenuOpen(m, false); }); }

/* Event delegation on document — decoupled from initial apply so a bad stored
   value can never leave the controls inert. Survives re-injected DOM. */
function bindControls() {
  if (root.dataset.themeBound === '1') return;
  root.dataset.themeBound = '1';
  document.addEventListener('click', (e) => {
    const trigger = e.target.closest?.('[data-theme-menu-trigger]');
    if (trigger) { const m = trigger.closest('[data-theme-menu]'); if (m) { const p = m.querySelector('[data-theme-menu-panel]'); const willOpen = p ? p.hidden : !m.classList.contains('is-open'); closeMenus(m); setMenuOpen(m, willOpen); return; } }
    const tBtn = e.target.closest?.('[data-theme-value]'); if (tBtn) { applyTheme(tBtn.dataset.themeValue, true); return; }
    const pBtn = e.target.closest?.('[data-palette-value]'); if (pBtn) { applyPalette(pBtn.dataset.paletteValue, true); return; }
    if (e.target.closest?.('[data-theme-menu]')) return;
    closeMenus(null);
  });
  document.addEventListener('input', (e) => {
    const a = e.target.closest?.('[data-theme-accent]'); if (a) { applyPalette(a.value, true); return; }
    const s = e.target.closest?.('[data-ui-scale]'); if (s) applyScale(s.value, true);
  });
  document.addEventListener('dblclick', (e) => {
    if (e.target.closest?.('[data-theme-accent]')) { resetAccent(); return; }
    if (e.target.closest?.('[data-ui-scale]')) applyScale(DEFAULT_SCALE, true);
  });
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeMenus(null); });
}

/* Cross-window sync: a detached popout receives main-window theme changes. */
function bindChannel() {
  if (!channel) return;
  channel.onmessage = ({ data }) => {
    if (!data) return;
    if (data.kind === 'theme') applyTheme(data.theme, false, false);
    else if (data.kind === 'accent') applyPalette(data.accent, false, data.theme, false);
    else if (data.kind === 'scale') applyScale(data.scale, false, false);
  };
}

export function initTheme() {
  try {
    const t = readTheme();
    applyPalette(readAccent(t), false, t, false);
    applyTheme(t, false, false);
    applyScale(readScale(), false, false);
    // shareable deep-link override: ?theme=hacker&seed=%2331d7ff
    const q = new URLSearchParams(location.search);
    const qt = q.get('theme'), qs = q.get('seed');
    if (qt && THEMES.has(qt)) applyTheme(qt, false, false);
    if (qs) applyPalette(qs, false, qt || readTheme(), false);
  } catch { /* defaults remain; controls still bind below */ }
  bindControls();
  bindChannel();
}
