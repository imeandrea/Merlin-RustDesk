// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

// Tema: automatico dal sistema operativo oppure fissato (chiaro/scuro).
// La scelta è una preferenza locale di questo computer.
export type ThemeChoice = "system" | "light" | "dark"
const KEY = "merlin-rustdesk-theme"
const media = window.matchMedia("(prefers-color-scheme: dark)")

export function getTheme(): ThemeChoice {
  try {
    const v = localStorage.getItem(KEY)
    if (v === "light" || v === "dark") return v
  } catch { /* storage non disponibile */ }
  return "system"
}

export function applyTheme() {
  const t = getTheme()
  document.documentElement.classList.toggle("dark", t === "dark" || (t === "system" && media.matches))
}

export function setTheme(t: ThemeChoice) {
  try { if (t === "system") localStorage.removeItem(KEY); else localStorage.setItem(KEY, t) } catch { /* nulla */ }
  applyTheme()
}

media.addEventListener("change", applyTheme)
