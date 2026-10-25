// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

import * as React from "react"
import { Channel, invoke } from "@tauri-apps/api/core"
import { ArrowLeftRightIcon, FolderIcon, KeyRoundIcon, MessageSquareIcon, MonitorIcon, PowerIcon, XIcon } from "lucide-react"

import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter,
  AlertDialogHeader, AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"
import { playChime } from "@/lib/chime"
import { ChatPanel, FilesPanel, type ChatLine } from "@/viewer-panels"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

// Finestra di visione: lo schermo di un altro computer (inversione di ruolo o
// «Controlla un PC»), con mouse e tastiera, chat, file, inverti ruoli,
// Ctrl+Alt+Canc e riavvio. Riquadri `[x y w h dw dh: u16 LE][RGBA]` da un canale
// Tauri, input con `viewer_input` (maschere del mouse di RustDesk).

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
  const [message, setMessage] = React.useState("Collegamento…")
  /** Il PC remoto chiede la password (connessione normale); wrong: quella data era sbagliata. */
  const [needPassword, setNeedPassword] = React.useState<{ wrong: boolean } | null>(null)
  const [password, setPassword] = React.useState("")
  const [host, setHost] = React.useState("")
  const [displays, setDisplays] = React.useState(1)
  const [current, setCurrent] = React.useState(0)
  const [cursor, setCursor] = React.useState("default")
  const cursors = React.useRef(new Map<string, string>())
  const buttons = React.useRef(0)
  const [platform, setPlatform] = React.useState("")
  const [panel, setPanel] = React.useState<"chat" | "files" | null>(null)
  const panelRef = React.useRef(panel); panelRef.current = panel
  const [chat, setChat] = React.useState<ChatLine[]>([])
  const [unread, setUnread] = React.useState(0)
  const [confirm, setConfirm] = React.useState<"switch" | "restart" | null>(null)
  /** Inversione chiesta da qui: la chiusura della sessione è l'altro PC che si ricollega. */
  const switching = React.useRef(false)
  const windows = platform.toLowerCase().includes("windows")

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
      setNeedPassword(null)
    }
    const onEvent = new Channel<ViewerEvent>()
    onEvent.onmessage = e => {
      switch (e.ev) {
        case "peer":
          setHost(String(e.hostname ?? ""))
          setPlatform(String(e.platform ?? ""))
          setDisplays(Number(e.displays ?? 1))
          setCurrent(Number(e.currentDisplay ?? 0))
          break
        case "switch_display": setCurrent(Number(e.display ?? 0)); break
        case "cursor": cursors.current.set(String(e.id), `url(data:image/png;base64,${e.png}) ${e.hotx} ${e.hoty}, default`); break
        case "cursor_id": setCursor(cursors.current.get(String(e.id)) ?? "default"); break
        case "need_password": setNeedPassword({ wrong: e.wrong === true }); break
        case "chat":
          setChat(c => [...c, { from: "peer", text: String(e.text ?? "") }])
          if (panelRef.current !== "chat") setUnread(n => n + 1)
          playChime()
          break
        case "msgbox":
          if (String(e.type).includes("error")) { setStatus("ended"); setMessage(String(e.text || e.title)) }
          // Attesa dell'accettazione sul PC remoto e simili.
          else if (e.text) setMessage(String(e.text))
          break
        case "switched_back": setStatus("ended"); setMessage("Il tecnico ha ripreso il controllo."); break
        case "ended":
          setStatus("ended")
          setMessage(m => (switching.current ? "Ruoli invertiti: ora l'altro computer vede e controlla questo."
            : m.startsWith("Collegamento") || m === "" ? "Visione terminata." : m))
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

  // Tastiera: tasti fisici (map mode) verso il PC remoto, tranne mentre si
  // scrive nei pannelli (chat, password dei file).
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (status !== "live" || confirm) return
      const t = e.target as HTMLElement | null
      if (t?.closest("input, textarea, [role=dialog]")) return
      e.preventDefault()
      input({ t: "key", code: e.code, down: e.type === "keydown" })
    }
    window.addEventListener("keydown", onKey)
    window.addEventListener("keyup", onKey)
    return () => { window.removeEventListener("keydown", onKey); window.removeEventListener("keyup", onKey) }
  }, [status, confirm])

  const togglePanel = (p: "chat" | "files") => {
    setPanel(cur => (cur === p ? null : p))
    if (p === "chat") setUnread(0)
  }
  const live = status === "live"
  const tool = (label: string, icon: React.ReactNode, onClick: () => void, opts: { pressed?: boolean; disabled?: boolean; badge?: number } = {}) => (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button size="icon" variant={opts.pressed ? "secondary" : "ghost"} className="relative size-7" aria-label={label} aria-pressed={opts.pressed}
          disabled={opts.disabled} onClick={onClick}>
          {icon}
          {!!opts.badge && <Badge className="absolute -top-1 -right-1 h-4 min-w-4 rounded-full px-1 text-[10px] tabular-nums">{opts.badge}</Badge>}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )

  return (
    <TooltipProvider>
    <div className="bg-background text-foreground flex h-screen flex-col select-none">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b pr-1.5 pl-3">
        <MonitorIcon className="text-muted-foreground size-4 shrink-0" />
        <span className="truncate text-sm font-medium">Controllo remoto</span>
        {host && <span className="text-muted-foreground truncate text-xs">· {host}</span>}
        <div className="ml-auto flex items-center gap-1">
          {displays > 1 && Array.from({ length: displays }, (_, i) => (
            <Button key={i} size="sm" variant={i === current ? "secondary" : "ghost"} className="h-7 px-2 tabular-nums"
              onClick={() => input({ t: "display", display: i })}>{i + 1}</Button>
          ))}
          {displays > 1 && <span className="bg-border mx-1 h-5 w-px" />}
          {tool("Chat", <MessageSquareIcon />, () => togglePanel("chat"), { pressed: panel === "chat", disabled: !live, badge: unread })}
          {tool("File", <FolderIcon />, () => togglePanel("files"), { pressed: panel === "files", disabled: !live })}
          {tool("Inverti ruoli: l'altro computer vede e controlla questo", <ArrowLeftRightIcon />, () => setConfirm("switch"), { disabled: !live })}
          {windows && tool("Ctrl+Alt+Canc", <span className="text-[10px] font-semibold">CAD</span>, () => input({ t: "ctrl_alt_del" }), { disabled: !live })}
          {tool("Riavvia il PC remoto", <PowerIcon />, () => setConfirm("restart"), { disabled: !live })}
          <span className="bg-border mx-1 h-5 w-px" />
          <Button size="sm" variant="ghost" className="h-7" onClick={() => void invoke("viewer_close")}><XIcon /> Chiudi</Button>
        </div>
      </div>
      <div className="flex min-h-0 flex-1">
      <div className="relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-hidden bg-black">
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
            {status === "connecting" && !needPassword && <Spinner />}
            {status === "connecting" && needPassword ? (
              <form className="flex w-72 flex-col gap-2" onSubmit={e => {
                e.preventDefault()
                if (!password) return
                input({ t: "login", password })
                setPassword("")
                setNeedPassword(null)
              }}>
                <span className="flex items-center gap-2 font-medium"><KeyRoundIcon className="size-4" />{needPassword.wrong ? "Password errata" : "Il PC remoto chiede la password"}</span>
                <div className="flex gap-2">
                  <Input type="password" autoFocus value={password} onChange={e => setPassword(e.target.value)} placeholder="Password del PC remoto" aria-invalid={needPassword.wrong} />
                  <Button type="submit" disabled={!password}>Connetti</Button>
                </div>
                <span className="text-muted-foreground text-xs">Oppure attendi che sul PC remoto accettino la richiesta.</span>
              </form>
            ) : (
              <span className="text-muted-foreground max-w-80 text-center">{message}</span>
            )}
          </div>
        )}
      </div>
      {panel && (
        <aside className="bg-background flex w-80 shrink-0 flex-col border-l">
          {panel === "chat"
            ? <ChatPanel lines={chat} onClose={() => setPanel(null)} onSend={text => { setChat(c => [...c, { from: "me", text }]); input({ t: "chat", text }) }} />
            : <FilesPanel windows={windows} onClose={() => setPanel(null)} />}
        </aside>
      )}
      </div>
      <AlertDialog open={confirm !== null} onOpenChange={o => { if (!o) setConfirm(null) }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{confirm === "switch" ? "Invertire i ruoli?" : "Riavviare il PC remoto?"}</AlertDialogTitle>
            <AlertDialogDescription>
              {confirm === "switch"
                ? `${host || "L'altro computer"} vedrà e potrà usare questo computer; questa finestra si chiude.`
                : `${host || "Il PC"} si riavvia subito; i programmi aperti si chiudono senza salvare.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Annulla</AlertDialogCancel>
            <AlertDialogAction onClick={() => {
              if (confirm === "switch") { switching.current = true; input({ t: "switch_sides" }) } else input({ t: "restart" })
              setConfirm(null)
            }}>{confirm === "switch" ? <><ArrowLeftRightIcon /> Inverti ruoli</> : <><PowerIcon /> Riavvia</>}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
    </TooltipProvider>
  )
}
