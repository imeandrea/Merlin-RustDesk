#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Build del client Merlin-RustDesk sulla VM Linux di build (root@192.168.40.95,
# sorgente sincronizzato con rsync in /root/merlin-rustdesk).
. /root/.cargo/env 2>/dev/null
cd /root/merlin-rustdesk/client && npm ci && VCPKG_ROOT=/opt/vcpkg npx tauri build --bundles appimage,deb
echo CLIENT_EXIT=$?
ls -la /root/merlin-rustdesk/target/release/bundle/appimage /root/merlin-rustdesk/target/release/bundle/deb 2>/dev/null
