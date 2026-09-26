/* ==========================================================================
   The curved tube — the page itself bent through the barrel, text intact.

   Two earlier versions taught the constraint. Bending the live page with an
   SVG feDisplacementMap broke the text, because Chrome samples that filter
   nearest-pixel at screen resolution. Bending only the glass left everything
   inside it flat. curved-glass-web (BROWN-FAMILY-SPORTS) had the method all
   along: never bend the DOM — draw the content into a canvas at 2x, upload it
   as a texture with LINEAR filtering, and bend the texture in a fragment
   shader. A glyph pulled half a pixel is blended, not chopped.

   Our content is an HTML page rather than hand-drawn canvas, so the page is
   drawn into the canvas by snapshot:

     1. The tube's page is cloned, serialised, and wrapped in an SVG
        foreignObject with every stylesheet on the page inlined — the fonts
        included as data: URLs, because an SVG image cannot fetch anything.
     2. The SVG is drawn into a canvas at up to 2x and uploaded as one tall
        texture, sized to what the GPU allows.
     3. Every frame, the shader draws the slice of that texture the reader is
        scrolled to, through the barrel, with the scanlines, rim, tracking
        band and glare drawn in the same pass.

   The real page stays underneath and keeps doing everything a page does: it
   scrolls, it is what a screen reader and a search engine read, and it takes
   every click — a click on the glass is mapped through the barrel to the
   element that is visibly under the pointer and forwarded there. A new
   snapshot is taken whenever the page changes (a theme chip, the TERM/BENCH
   switch, a register tab) and when the pointer moves onto something
   clickable, so hover still shows. A snapshot costs roughly 100 ms.

   It runs only with the tube on, in glass, on a pane at least 600px wide,
   with WebGL2. Anywhere else td-shell.js's flat glass overlay stands in.
   ========================================================================== */
/* ---------------------------------------------------------------- TD_SNAP
   Drawing a piece of our own page into an image, shared by the tube below
   and by anything else that bends part of the page (assets/td-panes.js). */
(function () {
  'use strict';
  var root = document.documentElement, linkPromise = null;

  function toDataUrl(url) {
    return fetch(url).then(function (r) { return r.arrayBuffer(); }).then(function (buf) {
      var bytes = new Uint8Array(buf), bin = '';
      for (var i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
      return 'data:font/woff2;base64,' + btoa(bin);
    });
  }

  /* Every stylesheet on the page, with the selectors that address the document
     root pointed at the snapshot's wrapper instead, :hover turned into a class
     the snapshot can set, and the woff2 files inlined. The linked sheets and
     their fonts are fetched once; the page's own <style> elements are read
     again on every snapshot, because a page may rewrite one (the docs paint a
     chosen theme's colours into a <style>), and a snapshot taken with the old
     colours would bend the wrong page. */
  var WOFF2 = /url\(\s*['"]?([^'")]+\.woff2)['"]?\s*\)/g;
  function css() {
    if (!linkPromise) {
      var links = Array.prototype.filter.call(document.querySelectorAll('link[rel="stylesheet"]'), function (l) { return l.href.indexOf(location.origin) === 0; });
      linkPromise = Promise.all(links.map(function (l) { return fetch(l.href).then(function (r) { return r.text(); }); })).then(function (parts) {
        var text = parts.join('\n'), fonts = {};
        text.replace(WOFF2, function (_, u) { fonts[u] = true; return _; });
        return Promise.all(Object.keys(fonts).map(function (u) { return toDataUrl(new URL(u, location.href).href).then(function (d) { fonts[u] = d; }); })).then(function () {
          return { text: text, fonts: fonts };
        });
      });
    }
    return linkPromise.then(function (base) {
      var styles = Array.prototype.map.call(document.querySelectorAll('style'), function (s) { return s.textContent; }).join('\n');
      var text = (base.text + '\n' + styles).replace(WOFF2, function (m, u) { return base.fonts[u] ? 'url(' + base.fonts[u] + ')' : m; });
      text = text.replace(/:root/g, '.snap-root')
                 .replace(/(^|[\s,}>(])(html|body)(?=[\s,{.:\[>)])/g, '$1.snap-root')
                 .replace(/:hover/g, '.snap-hover');
      /* the tube is laid out as a plain block of its full height here */
      text += '\n.snap-root #tube{position:static!important;overflow:visible!important;height:auto!important;filter:none!important;scroll-behavior:auto!important}';
      return text.replace(/&/g, '&amp;').replace(/</g, '&lt;');
    });
  }

  /* One element, at its own laid-out size, as an image. Its outer margin is
     dropped, so the image starts at its border box. */
  function element(el) {
    var W = el.offsetWidth, H = el.offsetHeight;
    return css().then(function (text) {
      var clone = el.cloneNode(true);
      clone.style.margin = '0';
      var html = new XMLSerializer().serializeToString(clone).replace(/<!--[\s\S]*?-->/g, '');
      var svg = '<svg xmlns="http://www.w3.org/2000/svg" width="' + W + '" height="' + H + '">' +
        '<foreignObject x="0" y="0" width="' + W + '" height="' + H + '">' +
        '<div xmlns="http://www.w3.org/1999/xhtml" class="snap-root" data-theme="' + root.dataset.theme + '" data-crt="' + root.dataset.crt + '"' +
        ' style="margin:0;width:' + W + 'px;height:' + H + 'px;overflow:hidden;background:none">' +
        '<style>' + text + '</style>' + html + '</div></foreignObject></svg>';
      return new Promise(function (ok, no) {
        var img = new Image();
        img.onload = function () { ok({ img: img, w: W, h: H }); };
        img.onerror = function () { no(new Error('the element snapshot did not load')); };
        img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
      });
    });
  }

  window.TD_SNAP = { css: css, element: element };
})();

(function () {
  'use strict';

  var root = document.documentElement;
  var tube = document.getElementById('tube');
  if (!tube || !window.WebGL2RenderingContext) return;

  /* The curve Parker settled on, in the same form td-shell.js's overlay
     uses, so the fallback and the real thing bend alike. */
  var CURVE = 0.39, K1 = CURVE * 0.6, K2 = CURVE * 0.25;
  var MIN_WIDTH = 600;
  var REDUCED = matchMedia('(prefers-reduced-motion: reduce)').matches;
  var CLICKABLE = 'a[href], button, label, summary, [data-td-toggle], [role="button"]';

  var cv = null, gl = null, tex = null, U = {}, MAX = 0, scene = null;
  var wallTex = null, wallVer = -1, hasWall = false;
  var total = 1, texScale = 1, gen = 0, busy = false, again = false, live = false, failed = false, raf = 0, timer = 0;
  var islandBuf = null;
  var hover = null, forwarding = false;

  function wanted() {
    return !failed && root.dataset.crt === 'on' && root.dataset.theme !== 'paper' && tube.clientWidth >= MIN_WIDTH;
  }

  /* ------------------------------------------------------------ WebGL2 */

  var VERT = '#version 300 es\nvoid main(){ vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }';
  var FRAG = [
    '#version 300 es',
    'precision highp float;',
    'uniform sampler2D tex;',
    'uniform sampler2D wall;',
    'uniform vec4 wallMap;',
    'uniform vec2 res;',
    'uniform float k1, k2, scroll, vp, total, time, motion, hasWall;',
    'out vec4 o;',
    /* the canonical barrel: a screen point samples from further out */
    'vec2 warp(vec2 uv){ vec2 c = uv - 0.5; float r2 = dot(c, c); return 0.5 + c * (1.0 + k1 * r2 + k2 * r2 * r2); }',
    'void main(){',
    '  vec2 uv0 = gl_FragCoord.xy / res; uv0.y = 1.0 - uv0.y;',
    '  vec2 uv = warp(uv0);',
    '  vec2 e = min(uv, 1.0 - uv);',
    '  float edge = smoothstep(0.0, 0.004, min(e.x, e.y));',
    '  if (edge <= 0.0) { o = vec4(0.0, 0.0, 0.0, 1.0); return; }',
    '  vec4 pg = texture(tex, vec2(uv.x, (uv.y * vp + scroll) / total));',
    /* the wall behind the page, when the page supplies one: it does not
       scroll, and it bends through the same barrel as everything on it */
    '  vec3 col = hasWall > 0.5 ? mix(texture(wall, uv * wallMap.xy + wallMap.zw).rgb, pg.rgb, pg.a) : pg.rgb;',
    /* scanlines every 4 CSS px of the bent coordinate, so they bow too */
    '  float s = fract(uv.y * vp / 4.0);',
    '  col *= 1.0 - 0.2 * smoothstep(0.3, 0.0, abs(s - 0.125));',
    /* the tracking band: sweeps down for 7 s of every 16, as the app does */
    '  float t = mod(time, 16.0) / 16.0;',
    '  if (motion > 0.5 && t < 0.44) {',
    '    float by = mix(-0.12, 1.08, t / 0.44);',
    '    col += vec3(0.02, 0.07, 0.035) * smoothstep(0.06, 0.0, abs(uv.y - by));',
    '  }',
    /* vignette, rim shadow, and the top-left glare the hacker theme calls screen_glare */
    '  float r2 = dot(uv - 0.5, uv - 0.5);',
    '  col *= 1.0 - 0.55 * r2;',
    '  col *= mix(0.55, 1.0, smoothstep(0.0, 0.035, min(e.x, e.y)));',
    '  col += vec3(0.045, 0.07, 0.05) * smoothstep(0.55, 0.0, length((uv0 - vec2(0.2, 0.1)) * vec2(1.0, 1.45)));',
    '  o = vec4(col * edge, 1.0);',
    '}'
  ].join('\n');

  function initGL() {
    cv = document.createElement('canvas');
    cv.className = 'td-tube';
    cv.setAttribute('aria-hidden', 'true');
    cv.hidden = true;
    document.body.appendChild(cv);
    gl = cv.getContext('webgl2', { antialias: false, alpha: false, preserveDrawingBuffer: false });
    if (!gl) return false;
    function sh(type, src) {
      var x = gl.createShader(type); gl.shaderSource(x, src); gl.compileShader(x);
      if (!gl.getShaderParameter(x, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(x) || 'shader');
      return x;
    }
    var prog = gl.createProgram();
    gl.attachShader(prog, sh(gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog) || 'link');
    gl.useProgram(prog);
    gl.bindVertexArray(gl.createVertexArray());
    ['res', 'k1', 'k2', 'scroll', 'vp', 'total', 'time', 'motion', 'hasWall', 'wallMap', 'tex', 'wall'].forEach(function (n) { U[n] = gl.getUniformLocation(prog, n); });
    function texture() {
      var t = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      return t;
    }
    /* the wall sits on unit 1; unit 0 is left active, because the snapshot and
       the islands bind the page's texture without naming a unit */
    gl.activeTexture(gl.TEXTURE1); wallTex = texture();
    gl.activeTexture(gl.TEXTURE0); tex = texture();
    gl.uniform1i(U.tex, 0); gl.uniform1i(U.wall, 1);
    MAX = gl.getParameter(gl.MAX_TEXTURE_SIZE);
    return true;
  }

  /* The canvas covers the tube's content box and stops short of its
     scrollbar, which stays flat, visible and draggable. */
  function place() {
    var r = tube.getBoundingClientRect(), d = Math.min(window.devicePixelRatio || 1, 2);
    var w = tube.clientWidth, h = tube.clientHeight;
    cv.style.left = (r.left + tube.clientLeft) + 'px';
    cv.style.top = (r.top + tube.clientTop) + 'px';
    cv.style.width = w + 'px';
    cv.style.height = h + 'px';
    cv.width = Math.max(2, Math.round(w * d));
    cv.height = Math.max(2, Math.round(h * d));
  }

  function draw(now) {
    gl.viewport(0, 0, cv.width, cv.height);
    gl.uniform2f(U.res, cv.width, cv.height);
    gl.uniform1f(U.k1, K1); gl.uniform1f(U.k2, K2);
    gl.uniform1f(U.scroll, tube.scrollTop);
    gl.uniform1f(U.vp, tube.clientHeight);
    gl.uniform1f(U.total, total);
    gl.uniform1f(U.time, now / 1000);
    gl.uniform1f(U.motion, REDUCED ? 0 : 1);
    wall();
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  /* The wall. A snapshot cannot carry what sits behind the page, so a page
     that has a wallpaper hands it over as window.TD_GLASS_WALL: a canvas of
     the wall exactly as it draws it (blurred, under its scrim), a version it
     bumps when that canvas changes, and map(rect), which says where the pane
     sits on the wall this frame, as [scale x, scale y, offset x, offset y] in
     the canvas's own 0–1 units, so a wall that grows and pans as the page
     scrolls grows and pans under the glass too. With no wall supplied (the
     info kiosk), the page's own pixels are all there is, as before. */
  function wall() {
    var W = window.TD_GLASS_WALL;
    if (!W || !W.canvas || !W.canvas.width) { gl.uniform1f(U.hasWall, 0); hasWall = false; return; }
    if (W.version !== wallVer) {
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, wallTex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, W.canvas);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, tex);
      wallVer = W.version;
    }
    var m = W.map(tube.getBoundingClientRect(), tube);
    gl.uniform4f(U.wallMap, m[0], m[1], m[2], m[3]);
    gl.uniform1f(U.hasWall, 1);
    hasWall = true;
  }
  function loop(now) {
    if (!live) return;                 // a detached loop must end when the tube goes
    if (!document.hidden) { islands(); draw(now); }
    raf = requestAnimationFrame(loop);
  }

  /* Live islands. A snapshot is a still, and a canvas serialises as an empty
     box, so anything that animates is drawn by the page into a
     <canvas data-glass-live> and copied into the texture here, over the
     snapshot's own pixels for that spot, whenever the page has drawn a new
     frame into it (it bumps canvas.__tdFrame). Only islands on screen are
     copied, and each copy is a few kilobytes, not a snapshot. */
  function islands() {
    if (!scene) return;
    var list = tube.querySelectorAll('canvas[data-glass-live]');
    if (!list.length) return;
    var tr = tube.getBoundingClientRect();
    var top0 = tr.top + tube.clientTop, left0 = tr.left + tube.clientLeft, vh = tube.clientHeight;
    for (var i = 0; i < list.length; i++) {
      var el = list[i], stamp = gen + ':' + (el.__tdFrame || 0);
      if (el.__tdUploaded === stamp) continue;
      var r = el.getBoundingClientRect();
      if (!r.width || r.bottom < top0 || r.top > top0 + vh) continue;
      var x = Math.round((r.left - left0) * texScale), y = Math.round((r.top - top0 + tube.scrollTop) * texScale);
      var w = Math.round(r.width * texScale), h = Math.round(r.height * texScale);
      if (x < 0 || y < 0 || x + w > scene.width || y + h > scene.height) continue;
      if (!islandBuf) islandBuf = document.createElement('canvas');
      islandBuf.width = w; islandBuf.height = h;
      var g = islandBuf.getContext('2d');
      g.drawImage(scene, x, y, w, h, 0, 0, w, h);
      g.drawImage(el, 0, 0, w, h);
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texSubImage2D(gl.TEXTURE_2D, 0, x, y, gl.RGBA, gl.UNSIGNED_BYTE, islandBuf);
      el.__tdUploaded = stamp;
    }
  }

  /* ---------------------------------------------------------- snapshot */

  var pageCss = window.TD_SNAP.css;

  function pathTo(el) {
    var p = [];
    while (el && el !== tube) { p.unshift(Array.prototype.indexOf.call(el.parentNode.children, el)); el = el.parentNode; }
    return el === tube ? p : null;
  }
  function follow(node, path) {
    for (var i = 0; node && i < path.length; i++) node = node.children[path[i]];
    return node;
  }

  function snapshot() {
    if (!gl) return;
    if (busy) { again = true; return; }
    busy = true;
    var W = tube.clientWidth, VW = document.documentElement.clientWidth;
    var H = Math.max(tube.scrollHeight, tube.clientHeight);
    var scale = Math.min(2, MAX / H, MAX / W);
    if (scale < 1) { busy = false; fallBack('the page is taller than this GPU can hold as one texture'); return; }

    pageCss().then(function (css) {
      /* the clone carries what the live DOM only holds as properties */
      var clone = tube.cloneNode(true);
      var a = tube.querySelectorAll('input'), b = clone.querySelectorAll('input');
      for (var i = 0; i < a.length; i++) { if (a[i].checked) b[i].setAttribute('checked', ''); else b[i].removeAttribute('checked'); }
      if (hover) {
        var hp = pathTo(hover), h = hp && follow(clone, hp);
        for (; h && h !== clone.parentNode; h = h.parentNode) if (h.classList) h.classList.add('snap-hover');
      }
      clone.style.width = W + 'px';
      var html = new XMLSerializer().serializeToString(clone).replace(/<!--[\s\S]*?-->/g, '');
      /* The SVG is as wide as the window so the page's media queries answer
         the same way they do on the live page; the tube sits at its left. */
      var svg = '<svg xmlns="http://www.w3.org/2000/svg" width="' + VW + '" height="' + H + '">' +
        '<foreignObject x="0" y="0" width="' + VW + '" height="' + H + '">' +
        '<div xmlns="http://www.w3.org/1999/xhtml" class="snap-root" data-theme="' + root.dataset.theme + '" data-crt="on"' +
        ' style="margin:0;width:' + W + 'px;height:' + H + 'px;overflow:hidden">' +
        '<style>' + css + '</style>' + html + '</div></foreignObject></svg>';
      return new Promise(function (ok, no) {
        var img = new Image();
        img.onload = function () { ok(img); };
        img.onerror = function () { no(new Error('the snapshot SVG did not load')); };
        img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
      });
    }).then(function (img) {
      if (!scene) scene = document.createElement('canvas');
      scene.width = Math.round(W * scale); scene.height = Math.round(H * scale);
      var g = scene.getContext('2d');
      g.drawImage(img, 0, 0, W, H, 0, 0, scene.width, scene.height);
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, scene);
      total = H;
      texScale = scene.width / W;
      gen++;                           // every island is copied again over the fresh snapshot
      busy = false;
      if (wanted()) goLive();
      window.__tdGlass = { live: live, width: W, height: H, scale: scale, max: MAX, get wall() { return hasWall; } };
      if (again) { again = false; snapshot(); }
    }).catch(function (e) {
      busy = false;
      fallBack(e && e.message);
    });
  }

  function schedule(ms) { clearTimeout(timer); timer = setTimeout(snapshot, ms); }

  /* ---------------------------------------------------- live and gone */

  function goLive() {
    if (live) return;
    live = true;
    cv.hidden = false;
    root.dataset.tube = 'gl';
    raf = requestAnimationFrame(loop);
  }
  function stand() {
    live = false;
    cancelAnimationFrame(raf);
    if (cv) cv.hidden = true;
    delete root.dataset.tube;
    root.classList.remove('tube-pointer');
    hover = null;
  }
  function fallBack(why) {
    failed = true;
    stand();
    if (window.console) console.warn('[td-glass] flat tube instead: ' + why);
  }

  function update() {
    if (!wanted()) { stand(); return; }
    if (!gl) {
      try { if (!initGL()) { fallBack('no WebGL2 context'); return; } }
      catch (e) { fallBack(e.message); return; }
    }
    place();
    schedule(0);
  }

  /* --------------------------------------------- the pointer, through it */

  /* screen point on the glass → the point of the live page drawn there */
  function throughGlass(x, y) {
    var r = cv.getBoundingClientRect();
    var u = (x - r.left) / r.width - 0.5, v = (y - r.top) / r.height - 0.5;
    var r2 = u * u + v * v, f = 1 + K1 * r2 + K2 * r2 * r2;
    var su = 0.5 + u * f, sv = 0.5 + v * f;
    if (su < 0 || su > 1 || sv < 0 || sv > 1) return null;
    return { x: r.left + su * r.width, y: r.top + sv * r.height };
  }

  /* Only the click the pointer made is redirected. A label answers a click
     by firing a second one at its input, keyboard activation fires one at
     the focused element, and scripts call click(): none of those came from
     where the pointer is, and redirecting them would cancel the very thing
     they were doing (a TERM label that never checks its radio). */
  var pressed = null;
  document.addEventListener('pointerdown', function (e) { pressed = e.isTrusted ? e.target : null; }, true);

  document.addEventListener('click', function (e) {
    if (!live || forwarding || !e.isTrusted || !pressed || !tube.contains(e.target)) return;
    if (e.target !== pressed && !e.target.contains(pressed)) return;
    pressed = null;
    var p = throughGlass(e.clientX, e.clientY);
    var t = p && document.elementFromPoint(p.x, p.y);
    if (t === e.target) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (!t || !tube.contains(t)) return;           // off the glass, or into the black corner
    forwarding = true;
    try { t.click(); } finally { forwarding = false; }
  }, true);

  var moveQueued = false, lastX = 0, lastY = 0;
  tube.addEventListener('mousemove', function (e) {
    if (!live) return;
    lastX = e.clientX; lastY = e.clientY;
    if (moveQueued) return;
    moveQueued = true;
    requestAnimationFrame(function () {
      moveQueued = false;
      var p = throughGlass(lastX, lastY);
      var t = p && document.elementFromPoint(p.x, p.y);
      var c = t && tube.contains(t) ? t.closest(CLICKABLE) : null;
      if (c === hover) return;
      hover = c;
      root.classList.toggle('tube-pointer', !!c);
      schedule(20);
    });
  }, { passive: true });
  tube.addEventListener('mouseleave', function () {
    if (!live || !hover) return;
    hover = null; root.classList.remove('tube-pointer'); schedule(20);
  });

  /* ---------------------------------------------- when to snapshot again */

  new MutationObserver(function () { if (gl && wanted()) schedule(60); })
    .observe(tube, { subtree: true, childList: true, characterData: true, attributes: true });
  tube.addEventListener('change', function () { if (live) schedule(20); });
  /* data-palette: the docs' Omarchy theme picker, whose colours the snapshot must carry */
  new MutationObserver(update).observe(root, { attributes: true, attributeFilter: ['data-crt', 'data-theme', 'data-palette'] });
  var rt = 0;
  addEventListener('resize', function () { clearTimeout(rt); rt = setTimeout(update, 150); });
  if (document.fonts) document.fonts.addEventListener('loadingdone', function () { if (live) schedule(30); });

  (document.fonts ? document.fonts.ready : Promise.resolve()).then(update);
  window.__tdGlassUpdate = update;
})();
