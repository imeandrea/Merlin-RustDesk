// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

import { createRoot } from "react-dom/client"
import App from "./App"
import Viewer from "./viewer"
import { applyTheme } from "./lib/theme"
import { loadBrand } from "./lib/brand"
import "./index.css"

applyTheme()
// Niente menu del tasto destro della webview (Ricarica, Ispeziona elemento): i
// menu sono quelli dell'app. Restano nei campi di testo (copia e incolla).
document.addEventListener("contextmenu", e => {
  if (!(e.target as HTMLElement | null)?.closest("input, textarea, [contenteditable=true]")) e.preventDefault()
})
const viewer = window.location.hash === "#viewer"
if (!viewer) document.documentElement.classList.add("fit")
// `#viewer`: finestra di visione dell'inversione di ruolo (processo a sé).
void loadBrand().finally(() => createRoot(document.getElementById("root")!).render(viewer ? <Viewer /> : <App />))
