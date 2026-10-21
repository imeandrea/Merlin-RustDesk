import { createRoot } from "react-dom/client"
import App from "./App"
import { applyTheme } from "./lib/theme"
import "./index.css"

applyTheme()
createRoot(document.getElementById("root")!).render(<App />)
