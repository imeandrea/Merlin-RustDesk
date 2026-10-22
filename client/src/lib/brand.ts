// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

import { invoke } from "@tauri-apps/api/core"

// Personalizzazione scritta da Merlin nel file del client (Windows/Linux).
// Su Mac, o senza personalizzazione, valgono i valori predefiniti.

export type ContactKind = "phone" | "email" | "web" | "whatsapp" | "hours" | "note"
export interface Contact { kind: ContactKind; value: string }
export interface Brand { name: string; color?: string; logo?: string; contacts: Contact[]
  /** Pulsante «Chiedi assistenza» (solo con il server Merlin, deciso dal client). */
  assistance: boolean }

export const brand: Brand = { name: "Merlin-RustDesk", contacts: [], assistance: false }

/** Legge la personalizzazione dal client e applica il colore. */
export async function loadBrand() {
  const b = await invoke<Partial<Brand> | null>("rd_brand").catch(() => null)
  if (!b) return
  brand.assistance = b.assistance === true
  if (typeof b.name === "string" && b.name.trim()) brand.name = b.name.trim()
  if (typeof b.logo === "string" && b.logo.startsWith("data:image/")) brand.logo = b.logo
  if (Array.isArray(b.contacts)) brand.contacts = b.contacts.filter(c => c && typeof c.value === "string" && c.value.trim()).slice(0, 4)
  if (typeof b.color === "string" && /^#[0-9a-f]{6}$/i.test(b.color)) {
    brand.color = b.color
    const style = document.createElement("style")
    style.textContent = `:root, .dark, [data-palette] { --primary: ${b.color} !important; --ring: ${b.color} !important; --primary-foreground: ${readableOn(b.color)} !important; }`
    document.head.appendChild(style)
  }
}

/** Testo bianco o nero, il più leggibile sul colore. */
function readableOn(hex: string) {
  const [r, g, b] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map(c => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.4 ? "#111111" : "#ffffff"
}
