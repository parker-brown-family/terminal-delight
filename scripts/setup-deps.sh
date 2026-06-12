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
echo "==> Vulkan check (should list the RTX 3080):"
vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverVersion' || \
  echo "!! vulkaninfo failed — NVIDIA Vulkan ICD may be missing; check nvidia driver install"

echo
echo "==> Done. Optional latency baseline: cargo install alacritty (or: sudo apt install alacritty)"
