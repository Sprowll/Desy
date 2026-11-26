#!/usr/bin/env bash
# System libraries GPUI needs to build on Debian/Ubuntu (X11, Wayland, Vulkan, fonts).
set -euo pipefail
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev \
  libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev \
  libasound2-dev libzstd-dev pkg-config
