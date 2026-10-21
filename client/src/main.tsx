import { createRoot } from "react-dom/client"
import App from "./App"
import { applyTheme } from "./lib/theme"
import { loadBrand } from "./lib/brand"
import "./index.css"

applyTheme()
void loadBrand().finally(() => createRoot(document.getElementById("root")!).render(<App />))
