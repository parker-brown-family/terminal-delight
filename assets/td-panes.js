/* ==========================================================================
   Per-pane glass: a drawing of the app, curved the way the app curves it.

   In Terminal Delight every pane is its own tube. The frames around the
   panes stay straight and the content inside each one bows, with its own
   dark corners and its own scanlines. A drawn window that is flat reads as a
   screenshot of some other terminal, so the hero window on the info kiosk is
   bent pane by pane here.

   The method is the tube's (td-glass.js), which is curved-glass-web's: never
   bend the DOM. The window is drawn into an image with TD_SNAP, uploaded at
   2x as a LINEAR texture, and a fragment shader draws it again with each
   [data-warp] rectangle bent through the barrel on its own. Anything marked
   [data-warp-flat] (the NEEDS ME list floats over the panes in the app) is
   drawn straight. The canvas sits exactly over the window; the HTML stays
   underneath for anyone reading with a screen reader, and shows through
   wherever WebGL2 is missing.

   The canvas is a [data-glass-live] island, so when the page tube is on the
   bent window is bent once more with the page, like a TD window on a CRT.

   Markup: <div class="shot"><div class="win" data-pane-warp>…</div></div>
   ========================================================================== */
(function () {
  'use strict';
  if (!window.WebGL2RenderingContext || !window.TD_SNAP) return;

  var MAX_PANES = 4, MAX_FLAT = 2;
  /* Close to the page's 0.39. At 0.5 the black rim inside each pane grew
     thicker than the app's own. */
  var CURVE = 0.36, K1 = CURVE * 0.6, K2 = CURVE * 0.25;

  var VERT = '#version 300 es\nvoid main(){ vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }';
  var FRAG = [
    '#version 300 es',
    'precision highp float;',
    'uniform sampler2D tex;',
    'uniform vec2 res, css;',
    'uniform vec4 panes[' + MAX_PANES + '];',
    'uniform float focus[' + MAX_PANES + '];',
    'uniform int np;',
    'uniform vec4 flats[' + MAX_FLAT + '];',
    'uniform int nf;',
    'uniform float k1, k2, dark;',
    'uniform vec3 glow;',
    'out vec4 o;',
    'vec2 warp(vec2 uv){ vec2 c = uv - 0.5; float r2 = dot(c, c); return 0.5 + c * (1.0 + k1 * r2 + k2 * r2 * r2); }',
    'bool inside(vec2 p, vec4 r){ return p.x >= r.x && p.y >= r.y && p.x <= r.x + r.z && p.y <= r.y + r.w; }',
    'void main(){',
    '  vec2 uv = gl_FragCoord.xy / res; uv.y = 1.0 - uv.y;',
    '  for (int i = 0; i < ' + MAX_FLAT + '; i++) { if (i >= nf) break; if (inside(uv, flats[i])) { o = texture(tex, uv); return; } }',
    '  for (int i = 0; i < ' + MAX_PANES + '; i++) {',
    '    if (i >= np) break;',
    '    vec4 r = panes[i];',
    '    if (!inside(uv, r)) continue;',
    '    vec2 lu = (uv - r.xy) / r.zw;',
    '    vec2 lw = warp(lu);',
    '    vec2 e = min(lw, 1.0 - lw);',
    '    float m = min(e.x, e.y);',
    '    float edge = smoothstep(0.0, 0.006, m);',
    '    vec3 col = texture(tex, r.xy + lw * r.zw).rgb;',
    /* scanlines every 3 CSS px of the bent coordinate, so they bow with it */
    '    float y = (r.y + lw.y * r.w) * css.y;',
    '    col *= 1.0 - dark * 0.22 * smoothstep(0.35, 0.0, abs(fract(y / 3.0) - 0.17));',
    '    float r2 = dot(lw - 0.5, lw - 0.5);',
    '    col *= 1.0 - dark * 0.55 * r2;',
    '    col *= mix(1.0 - dark * 0.45, 1.0, smoothstep(0.0, 0.05, m));',
    '    col += glow * focus[i] * 0.2 * smoothstep(0.07, 0.0, m) * edge;',
    '    vec3 off = mix(vec3(0.84, 0.86, 0.88), vec3(0.0), dark);',
    '    o = vec4(mix(off, col, edge), 1.0);',
    '    return;',
    '  }',
    '  o = texture(tex, uv);',
    '}'
  ].join('\n');

  function hex(c) {
    var m = /#([0-9a-f]{6})/i.exec(c || '');
    if (!m) return [0.13, 0.77, 0.37];
    var n = parseInt(m[1], 16);
    return [(n >> 16 & 255) / 255, (n >> 8 & 255) / 255, (n & 255) / 255];
  }

  function setup(win) {
    var host = win.parentNode;
    var cv = document.createElement('canvas');
    cv.className = 'pane-warp';
    cv.setAttribute('aria-hidden', 'true');
    cv.setAttribute('data-glass-live', '');
    cv.hidden = true;
    host.appendChild(cv);
    var gl = cv.getContext('webgl2', { alpha: true, premultipliedAlpha: true, preserveDrawingBuffer: true, antialias: false });
    if (!gl) { cv.remove(); return; }

    var U = {}, tex, scene = document.createElement('canvas');
    try {
      var sh = function (type, src) {
        var x = gl.createShader(type); gl.shaderSource(x, src); gl.compileShader(x);
        if (!gl.getShaderParameter(x, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(x) || 'shader');
        return x;
      };
      var prog = gl.createProgram();
      gl.attachShader(prog, sh(gl.VERTEX_SHADER, VERT));
      gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FRAG));
      gl.linkProgram(prog);
      if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog) || 'link');
      gl.useProgram(prog);
      gl.bindVertexArray(gl.createVertexArray());
      ['tex', 'res', 'css', 'panes', 'focus', 'np', 'flats', 'nf', 'k1', 'k2', 'dark', 'glow'].forEach(function (n) {
        U[n] = gl.getUniformLocation(prog, n) || gl.getUniformLocation(prog, n + '[0]');
      });
      tex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
    } catch (e) {
      if (window.console) console.warn('[td-panes] flat window instead: ' + e.message);
      cv.remove();
      return;
    }
    var MAX = gl.getParameter(gl.MAX_TEXTURE_SIZE);

    /* each marked rectangle, relative to the window and normalised to it */
    function rects(sel, limit) {
      var box = win.getBoundingClientRect(), out = [], flags = [];
      win.querySelectorAll(sel).forEach(function (el) {
        var r = el.getBoundingClientRect();
        if (out.length / 4 >= limit || !r.width || !r.height) return;
        out.push((r.left - box.left) / box.width, (r.top - box.top) / box.height, r.width / box.width, r.height / box.height);
        flags.push(el.hasAttribute('data-focus') ? 1 : 0);
      });
      return { list: out, flags: flags, n: flags.length };
    }

    var busy = false, again = false, timer = 0;
    function render() {
      if (busy) { again = true; return; }
      var W = win.offsetWidth, H = win.offsetHeight;
      if (!W || !H) return;
      busy = true;
      var d = Math.min(window.devicePixelRatio || 1, 2);
      window.TD_SNAP.element(win).then(function (s) {
        var scale = Math.min(2, MAX / s.w, MAX / s.h);
        scene.width = Math.round(s.w * scale); scene.height = Math.round(s.h * scale);
        var g = scene.getContext('2d');
        g.clearRect(0, 0, scene.width, scene.height);
        g.drawImage(s.img, 0, 0, s.w, s.h, 0, 0, scene.width, scene.height);

        cv.style.width = W + 'px'; cv.style.height = H + 'px';
        cv.width = Math.round(W * d); cv.height = Math.round(H * d);

        var p = rects('[data-warp]', MAX_PANES), f = rects('[data-warp-flat]', MAX_FLAT);
        var panes = new Float32Array(MAX_PANES * 4), focus = new Float32Array(MAX_PANES), flats = new Float32Array(MAX_FLAT * 4);
        panes.set(p.list); focus.set(p.flags); flats.set(f.list);
        var light = win.getAttribute('data-wear') === 'quiet-command';

        gl.viewport(0, 0, cv.width, cv.height);
        gl.bindTexture(gl.TEXTURE_2D, tex);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, scene);
        gl.uniform1i(U.tex, 0);
        gl.uniform2f(U.res, cv.width, cv.height);
        gl.uniform2f(U.css, W, H);
        gl.uniform4fv(U.panes, panes);
        gl.uniform1fv(U.focus, focus);
        gl.uniform1i(U.np, p.n);
        gl.uniform4fv(U.flats, flats);
        gl.uniform1i(U.nf, f.n);
        gl.uniform1f(U.k1, K1); gl.uniform1f(U.k2, K2);
        gl.uniform1f(U.dark, light ? 0 : 1);
        var gc = hex(getComputedStyle(win).getPropertyValue('--w-acc'));
        gl.uniform3f(U.glow, gc[0], gc[1], gc[2]);
        gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT);
        gl.drawArrays(gl.TRIANGLES, 0, 3);

        cv.hidden = false;
        win.classList.add('pane-warped');
        cv.__tdFrame = (cv.__tdFrame || 0) + 1;       // tells the page tube to copy it in
        busy = false;
        if (again) { again = false; render(); }
      }).catch(function (e) {
        busy = false;
        cv.hidden = true;
        if (window.console) console.warn('[td-panes] flat window instead: ' + (e && e.message));
      });
    }
    function schedule(ms) { clearTimeout(timer); timer = setTimeout(render, ms); }

    new MutationObserver(function () { schedule(40); }).observe(win, { attributes: true, attributeFilter: ['data-wear'] });
    new MutationObserver(function () { schedule(40); }).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'data-palette'] });
    if ('ResizeObserver' in window) new ResizeObserver(function () { schedule(120); }).observe(win);
    (document.fonts ? document.fonts.ready : Promise.resolve()).then(function () { schedule(0); });
  }

  /* A page that draws its window later (the docs, when their example window is
     opened) calls setup on it once it is in the document. */
  window.TD_PANES = { setup: setup };
  document.querySelectorAll('[data-pane-warp]').forEach(setup);
})();
