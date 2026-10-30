// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

fn main() {
    // Sidecar without UI: inside Merlin's bundle macOS would give it a Dock icon.
    #[cfg(target_os = "macos")]
    librustdesk::platform::macos::hide_dock();
    librustdesk::merlin::run_bridge();
}
