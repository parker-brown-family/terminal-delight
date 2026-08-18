#!/usr/bin/env bash
# terminal-delight — G0a build-dependency setup (Ubuntu 22.04)
# Package list sourced from Zed's script/linux (R1 research, docs/PLAN.md §2).
set -euo pipefail

echo "==> Installing GPUI/Vulkan build deps (sudo will prompt)…"
sudo apt install -y \
  libxkbcommon-x11-dev libx11-xcb-dev libwayland-dev \
  libvulkan1 vulkan-tools \
  libfontconfig-dev libasound2-dev libssl-dev libzstd-dev libsqlite3-dev \
  cmake clang lld libstdc++-12-dev

echo
echo "==> GPU check (any Vulkan-capable device — NVIDIA, AMD or Intel):"
if devices="$(vulkaninfo --summary 2>/dev/null | grep -E 'deviceName')" && [ -n "$devices" ]; then
  echo "$devices"
  echo "   Vulkan looks good. terminal-delight prefers Vulkan and (via wgpu) can"
  echo "   fall back to GL if needed."
else
  echo "!! No Vulkan device reported by vulkaninfo. terminal-delight will still try"
  echo "   GL via wgpu, but for best results install your GPU's Vulkan ICD:"
  echo "     NVIDIA  -> the proprietary driver ships it"
  echo "     AMD/Intel -> mesa-vulkan-drivers"
  echo "   On a hybrid/Optimus laptop you may need to force the discrete GPU"
  echo "   (e.g. DRI_PRIME=1, or your vendor's GPU-offload launcher)."
fi

echo
echo "==> Done. Optional latency baseline: cargo install alacritty (or: sudo apt install alacritty)"
