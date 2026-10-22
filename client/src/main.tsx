// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

import { createRoot } from "react-dom/client"
import App from "./App"
import { applyTheme } from "./lib/theme"
import { loadBrand } from "./lib/brand"
import "./index.css"

applyTheme()
void loadBrand().finally(() => createRoot(document.getElementById("root")!).render(<App />))
