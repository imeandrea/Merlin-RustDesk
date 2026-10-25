// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

import * as React from "react"
import { Channel, invoke } from "@tauri-apps/api/core"
import { MonitorIcon, XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

// Finestra di visione per l'inversione di ruolo: lo schermo del computer del
// tecnico, con mouse e tastiera. Riquadri `[x y w h dw dh: u16 LE][RGBA]` da un
// canale Tauri, input con `viewer_input` (maschere del mouse di RustDesk).

const MOUSE_MOVE = 0, MOUSE_DOWN = 1, MOUSE_UP = 2, MOUSE_WHEEL = 3
/** Pulsante del browser (0 sinistro, 1 centrale, 2 destro) → bit di RustDesk. */
const buttonBit = (b: number) => (b === 0 ? 0x01 : b === 1 ? 0x04 : 0x02)
/** Pixel della rotella per uno scatto. */
const WHEEL_STEP = 100

type ViewerEvent = { ev: string; [k: string]: unknown }

const input = (v: Record<string, unknown>) => { void invoke("viewer_input", { input: v }).catch(() => {}) }

export default function Viewer() {
  const canvasRef = React.useRef<HTMLCanvasElement>(null)
  const [status, setStatus] = React.useState<"connecting" | "live" | "ended">("connecting")
  const [message, setMessage] = React.useState("Collegamento al computer del tecnico…")
  const [host, setHost] = React.useState("")
  const [displays, setDisplays] = React.useState(1)
  const [current, setCurrent] = React.useState(0)
  const [cursor, setCursor] = React.useState("default")
  const cursors = React.useRef(new Map<string, string>())
  const buttons = React.useRef(0)

  React.useEffect(() => {
    const onTile = new Channel<ArrayBuffer>()
    onTile.onmessage = buf => {
      const c = canvasRef.current
      const ctx = c?.getContext("2d")
      if (!c || !ctx) return
      const v = new DataView(buf)
      const [x, y, w, h, dw, dh] = [0, 2, 4, 6, 8, 10].map(o => v.getUint16(o, true))
      if (c.width !== dw || c.height !== dh) { c.width = dw; c.height = dh }
      ctx.putImageData(new ImageData(new Uint8ClampedArray(buf, 12, w * h * 4), w, h), x, y)
      setStatus("live")
    }
    const onEvent = new Channel<ViewerEvent>()
    onEvent.onmessage = e => {
      switch (e.ev) {
        case "peer":
          setHost(String(e.hostname ?? ""))
          setDisplays(Number(e.displays ?? 1))
          setCurrent(Number(e.currentDisplay ?? 0))
          break
        case "switch_display": setCurrent(Number(e.display ?? 0)); break
        case "cursor": cursors.current.set(String(e.id), `url(data:image/png;base64,${e.png}) ${e.hotx} ${e.hoty}, default`); break
        case "cursor_id": setCursor(cursors.current.get(String(e.id)) ?? "default"); break
        case "msgbox":
          if (String(e.type).includes("error")) { setStatus("ended"); setMessage(String(e.text || e.title)) }
          break
        case "switched_back": setStatus("ended"); setMessage("Il tecnico ha ripreso il controllo."); break
        case "ended":
          setStatus("ended")
          setMessage(m => (m.startsWith("Collegamento") || m === "" ? "Visione terminata." : m))
          window.setTimeout(() => void invoke("viewer_close"), 3000)
          break
      }
    }
    invoke("viewer_start", { onTile, onEvent }).catch(e => { setStatus("ended"); setMessage(String(e)) })
  }, [])

  const toFb = (e: { clientX: number; clientY: number }) => {
    const c = canvasRef.current
    if (!c) return { x: 0, y: 0 }
    const r = c.getBoundingClientRect()
    return {
      x: Math.max(0, Math.min(c.width - 1, Math.round((e.clientX - r.left) * (c.width / (r.width || 1))))),
      y: Math.max(0, Math.min(c.height - 1, Math.round((e.clientY - r.top) * (c.height / (r.height || 1))))),
    }
  }
  const mods = (e: React.MouseEvent | MouseEvent) => ({ alt: e.altKey, ctrl: e.ctrlKey, shift: e.shiftKey, command: e.metaKey })

  // Rotella: listener non passivo per bloccare lo scorrimento; scatti come RustDesk.
  React.useEffect(() => {
    const c = canvasRef.current
    if (!c) return
    let acc = 0
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      acc += -(e.deltaMode === 1 ? e.deltaY * 40 : e.deltaY)
      const steps = Math.trunc(acc / WHEEL_STEP)
      if (!steps) return
      acc -= steps * WHEEL_STEP
      input({ t: "mouse", mask: MOUSE_WHEEL, x: 0, y: steps })
    }
    c.addEventListener("wheel", onWheel, { passive: false })
    return () => c.removeEventListener("wheel", onWheel)
  }, [])

  // Tastiera: tasti fisici (map mode) verso il computer del tecnico.
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (status !== "live") return
      e.preventDefault()
      input({ t: "key", code: e.code, down: e.type === "keydown" })
    }
    window.addEventListener("keydown", onKey)
    window.addEventListener("keyup", onKey)
    return () => { window.removeEventListener("keydown", onKey); window.removeEventListener("keyup", onKey) }
  }, [status])

  return (
    <div className="bg-background text-foreground flex h-screen flex-col select-none">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b px-3">
        <MonitorIcon className="text-muted-foreground size-4 shrink-0" />
        <span className="truncate text-sm font-medium">Computer del tecnico</span>
        {host && <span className="text-muted-foreground truncate text-xs">· {host}</span>}
        <div className="ml-auto flex items-center gap-1">
          {displays > 1 && Array.from({ length: displays }, (_, i) => (
            <Button key={i} size="sm" variant={i === current ? "secondary" : "ghost"} className="h-7 px-2 tabular-nums"
              onClick={() => input({ t: "display", display: i })}>{i + 1}</Button>
          ))}
          <Button size="sm" variant="ghost" className="h-7" onClick={() => void invoke("viewer_close")}><XIcon /> Chiudi</Button>
        </div>
      </div>
      <div className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden bg-black">
        <canvas ref={canvasRef} tabIndex={0} style={{ cursor }}
          className={cn("max-h-full max-w-full object-contain outline-none", status !== "live" && "invisible")}
          onContextMenu={e => e.preventDefault()}
          onPointerMove={e => { const p = toFb(e); input({ t: "mouse", mask: MOUSE_MOVE | (buttons.current << 3), ...p, ...mods(e) }) }}
          onPointerDown={e => {
            e.currentTarget.setPointerCapture(e.pointerId); e.currentTarget.focus()
            const b = buttonBit(e.button); buttons.current |= b
            input({ t: "mouse", mask: MOUSE_DOWN | (b << 3), ...toFb(e), ...mods(e) })
          }}
          onPointerUp={e => {
            const b = buttonBit(e.button); buttons.current &= ~b
            input({ t: "mouse", mask: MOUSE_UP | (b << 3), ...toFb(e), ...mods(e) })
          }} />
        {status !== "live" && (
          <div className="bg-background absolute inset-0 flex flex-col items-center justify-center gap-3 text-sm">
            {status === "connecting" && <Spinner />}
            <span className="text-muted-foreground max-w-80 text-center">{message}</span>
          </div>
        )}
      </div>
    </div>
  )
}
