// Q3. Ask the browser's WebGPU adapter for maxTextureDimension2D, under several flag sets,
// and report WHICH adapter answered: headless Chromium falls back to a software adapter,
// whose limits say nothing about the GPU TD renders on.
import { launch } from './lib.mjs';

const configs = {
  headlessDefault: ['--enable-unsafe-webgpu'],
  headlessVulkan: ['--enable-unsafe-webgpu', '--enable-features=Vulkan,VulkanFromANGLE,DefaultANGLEVulkan', '--use-angle=vulkan', '--ignore-gpu-blocklist', '--enable-gpu'],
};

const PROBE = async () => {
  if (!navigator.gpu) return { error: 'navigator.gpu missing' };
  const out = {};
  for (const pref of ['high-performance', 'low-power']) {
    const a = await navigator.gpu.requestAdapter({ powerPreference: pref });
    if (!a) { out[pref] = null; continue; }
    const info = a.info || (a.requestAdapterInfo ? await a.requestAdapterInfo() : {});
    out[pref] = {
      vendor: info.vendor, architecture: info.architecture, device: info.device, description: info.description,
      isFallbackAdapter: a.isFallbackAdapter ?? info.isFallbackAdapter,
      maxTextureDimension2D: a.limits.maxTextureDimension2D,
      maxBufferSize: a.limits.maxBufferSize,
    };
  }
  return out;
};

for (const [name, args] of Object.entries(configs)) {
  try {
    const b = await launch(args);
    const p = await b.newPage();
    await p.goto('file:///tmp/claude-1000/td-snapshot-spike/probe.html');
    // WebGPU needs a secure context: about:blank is not one, a file:// page is.

    let r;
    try { r = await p.evaluate(PROBE); } catch (e) { r = { error: String(e).slice(0, 200) }; }
    console.log(name, JSON.stringify(r));
    await b.close();
  } catch (e) { console.log(name, 'launch failed', String(e).slice(0, 200)); }
}
