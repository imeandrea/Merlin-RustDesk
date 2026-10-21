import * as React from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { Toaster, toast } from "sonner"
import {
  AppWindowIcon, CheckIcon, ClipboardIcon, CopyIcon, EyeIcon, EyeOffIcon, FolderIcon, GlobeIcon, HeadsetIcon,
  KeyRoundIcon, type LucideIcon, MessageSquareIcon, MonitorIcon, MousePointer2Icon, PowerIcon,
  RefreshCwIcon, SendIcon, PhoneIcon, MailIcon, MessageCircleIcon, ClockIcon, InfoIcon, SettingsIcon, LaptopIcon, SunIcon, MoonIcon, ShieldCheckIcon, SparklesIcon, WifiOffIcon, XIcon,
} from "lucide-react"

import logo from "@/assets/logo.png"
import { brand, type ContactKind } from "@/lib/brand"
import { cn } from "@/lib/utils"
import { getTheme, setTheme, type ThemeChoice } from "@/lib/theme"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Spinner } from "@/components/ui/spinner"
import { Switch } from "@/components/ui/switch"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"

// Client di assistenza remota Merlin-RustDesk. Due finestre:
// - principale: ID, password e stato del collegamento al server;
// - connessione in arrivo (#cm, processo --cm): accetta/rifiuta, sessione
//   in corso, chat. Gli eventi arrivano dal nucleo (merlin_client.rs).

type Tone = "emerald" | "sky" | "violet" | "amber" | "rose" | "primary"
const TONE: Record<Tone, string> = {
  emerald: "bg-emerald-400/15 text-emerald-600 dark:text-emerald-300",
  sky: "bg-sky-400/15 text-sky-600 dark:text-sky-300",
  violet: "bg-violet-400/15 text-violet-600 dark:text-violet-300",
  amber: "bg-amber-400/20 text-amber-600 dark:text-amber-300",
  rose: "bg-rose-400/15 text-rose-600 dark:text-rose-300",
  primary: "bg-primary/12 text-primary",
}

/** Icona, colore ed etichetta di ogni tipo di contatto. */
const CONTACT: Record<ContactKind, { icon: LucideIcon; tone: Tone; label: string }> = {
  phone: { icon: PhoneIcon, tone: "emerald", label: "Telefono" },
  email: { icon: MailIcon, tone: "sky", label: "Email" },
  web: { icon: GlobeIcon, tone: "violet", label: "Sito" },
  whatsapp: { icon: MessageCircleIcon, tone: "emerald", label: "WhatsApp" },
  hours: { icon: ClockIcon, tone: "amber", label: "Orari" },
  note: { icon: InfoIcon, tone: "primary", label: "Nota" },
}

const groupId = (id: string) => id.replace(/(\d{3})(?=\d)/g, "$1 ")
const copyText = (text: string, what: string) => { void navigator.clipboard.writeText(text).then(() => toast.success(`${what} copiato`)) }

export default function App() {
  const cm = window.location.hash === "#cm"
  return (
    <TooltipProvider>
      {cm ? <ConnectionWindow /> : <MainWindow />}
      <Toaster position="top-center" richColors />
    </TooltipProvider>
  )
}

// ── Finestra principale ─────────────────────────────────────────────────────

type InstallState = "unsupported" | "portable" | "installed"
interface Info { id: string; status: number; password: string; permanentSet: boolean; version: string; install: InstallState }

function MainWindow() {
  const conn = useConnections()
  const active = conn.client
  const [info, setInfo] = React.useState<Info | null>(null)
  const [settingsOpen, setSettingsOpen] = React.useState(false)
  const refresh = React.useCallback(() => { void invoke<Info>("rd_info").then(setInfo).catch(() => {}) }, [])
  React.useEffect(() => {
    refresh()
    const t = setInterval(refresh, 1000)
    return () => clearInterval(t)
  }, [refresh])
  // Installazione in corso: appena il client installato c'è, si apre lui e
  // questa copia (quella scaricata) si chiude.
  const [installing, setInstalling] = React.useState(false)
  React.useEffect(() => {
    if (installing && info?.install === "installed") void invoke("rd_exit")
  }, [installing, info?.install])
  React.useEffect(() => {
    if (!installing) return
    const t = setTimeout(() => setInstalling(false), 90_000)
    return () => clearTimeout(t)
  }, [installing])
  const online = (info?.status ?? 0) > 0
  const offline = (info?.status ?? 0) < 0

  return (
    <div className="bg-background text-foreground relative flex h-svh flex-col">
      <Header status={info?.status ?? 0} />
      {/* Mai scorrimento: tutto entra nella finestra (altezza minima della finestra). */}
      <main className="min-h-0 flex-1 overflow-hidden">
        <div className="flex h-full flex-col justify-center gap-5 px-5 py-4">
          {active?.authorized ? (
            <Session client={active} since={conn.since[active.id] ?? Date.now()} chat={conn.chat[active.id] ?? []} compact
              onSend={text => conn.actions.send(active, text)} onEnd={() => conn.actions.close(active)} />
          ) : offline ? (
            <Hero tiles={[["rose", WifiOffIcon], ["amber", RefreshCwIcon], ["sky", GlobeIcon]]}
              title="Nessuna connessione" text="Il computer non raggiunge il server di assistenza. Controlla Internet: si ricollega da solo." />
          ) : (
            <>
              <Hero tiles={[["sky", MonitorIcon], ["primary", HeadsetIcon], ["emerald", ShieldCheckIcon]]}
                title={online ? "Pronto per l'assistenza" : "Collegamento al server…"} />
              <Credentials id={info?.id ?? ""} password={info?.password ?? ""}
                onNewPassword={() => { void invoke("rd_new_password").then(() => { setTimeout(refresh, 300); toast("Nuova password generata") }) }} />
            </>
          )}
          {!active?.authorized && (brand.contacts.length === 0 ? <Guide /> : <Contacts />)}
        </div>
      </main>
      {/* Richiesta di collegamento: pannello dal basso, non modale (ID e password restano visibili). */}
      {active && !active.authorized && (
        <RequestPanel client={active} onAccept={() => conn.actions.accept(active)} onReject={() => conn.actions.close(active)} />
      )}
      <footer className="flex h-10 shrink-0 items-center gap-2 border-t px-2">
        <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => setSettingsOpen(true)}><SettingsIcon /> Accesso non presidiato</Button>
        <ThemeSwitch className="ml-auto" />
        <span className="text-muted-foreground pr-2 text-[11px] tabular-nums">{info?.version ? `v${info.version}` : ""}</span>
      </footer>
      <UnattendedDialog open={settingsOpen} onOpenChange={setSettingsOpen} permanentSet={!!info?.permanentSet} onSaved={refresh}
        install={info?.install ?? "unsupported"} installing={installing}
        onInstall={() => {
          setInstalling(true)
          void invoke("rd_install").catch(e => { setInstalling(false); toast.error(`Installazione non avviata: ${e}`) })
        }}
        onUninstall={() => { void invoke("rd_uninstall").catch(e => toast.error(`Disinstallazione non riuscita: ${e}`)) }} />
    </div>
  )
}

/** Barra in alto: logo, nome e stato (in un solo punto). */
function Header({ status }: { status: number }) {
  const s = status > 0 ? { text: "Online", cls: "text-emerald-600 dark:text-emerald-400", dot: "bg-emerald-500", ping: true }
    : status < 0 ? { text: "Offline", cls: "text-destructive", dot: "bg-destructive", ping: false }
    : { text: "Connessione…", cls: "text-amber-600 dark:text-amber-400", dot: "bg-amber-500", ping: true }
  return (
    <header data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-2.5 border-b px-4">
      <img src={brand.logo ?? logo} alt="" className="size-7 shrink-0 object-contain" draggable={false} />
      <span className="min-w-0 flex-1 truncate font-[JetBrains_Mono,ui-monospace,monospace] text-sm font-bold tracking-tight">{brand.name}</span>
      <span className={cn("flex items-center gap-1.5 text-xs", s.cls)}>
        <span className="relative flex size-2">
          {s.ping && <span className={cn("absolute inline-flex size-full animate-ping rounded-full opacity-50", s.dot)} />}
          <span className={cn("relative inline-flex size-2 rounded-full", s.dot)} />
        </span>
        {s.text}
      </span>
    </header>
  )
}

/** Tessere decorative a ventaglio (come nelle note degli host di Merlin). */
function Tiles({ tiles, size = "md" }: { tiles: Array<[Tone, LucideIcon]>; size?: "md" | "lg" }) {
  const pos = ["-rotate-12 translate-x-3 translate-y-1 delay-150", "z-10 backdrop-blur-sm", "rotate-12 -translate-x-3 translate-y-1 delay-300"]
  return (
    <div className="flex items-end justify-center">
      {tiles.map(([tone, Icon], i) => (
        <div key={i} className={cn("ring-background flex items-center justify-center rounded-2xl ring-4",
          size === "lg" ? "size-16" : "size-13",
          "animate-in fade-in zoom-in-75 slide-in-from-bottom-2 fill-mode-both duration-500 motion-reduce:animate-none", TONE[tone], pos[i])}>
          <Icon className={size === "lg" ? "size-7" : "size-6"} />
        </div>
      ))}
    </div>
  )
}

function Hero({ tiles, title, text }: { tiles: Array<[Tone, LucideIcon]>; title: string; text?: string }) {
  return (
    <div className="flex flex-col items-center gap-4 text-center">
      <Tiles tiles={tiles} />
      <div className="space-y-1">
        <div className="text-base font-semibold">{title}</div>
        {text && <p className="text-muted-foreground mx-auto max-w-72 text-xs/relaxed">{text}</p>}
      </div>
    </div>
  )
}

function Credentials({ id, password, onNewPassword }: { id: string; password: string; onNewPassword: () => void }) {
  const [show, setShow] = React.useState(false)
  return (
    <div className="grid gap-2">
      <div className="bg-muted/60 flex items-center gap-2 rounded-xl px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-muted-foreground text-[11px] font-medium tracking-wide uppercase">ID</div>
          <div className="font-mono text-[28px] leading-tight font-semibold tracking-wide tabular-nums select-text">{id ? groupId(id) : "—"}</div>
        </div>
        <IconButton label="Copia l'ID" disabled={!id} onClick={() => copyText(id, "ID")}><CopyIcon /></IconButton>
      </div>
      <div className="bg-muted/60 flex items-center gap-1 rounded-xl px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-muted-foreground text-[11px] font-medium tracking-wide uppercase">Password</div>
          <div className="font-mono text-lg leading-tight tracking-widest select-text">{!password ? "—" : show ? password : "••••••••"}</div>
        </div>
        <IconButton label={show ? "Nascondi" : "Mostra"} disabled={!password} onClick={() => setShow(v => !v)}>{show ? <EyeOffIcon /> : <EyeIcon />}</IconButton>
        <IconButton label="Copia la password" disabled={!password} onClick={() => copyText(password, "Password")}><CopyIcon /></IconButton>
        <IconButton label="Nuova password" onClick={onNewPassword}><RefreshCwIcon /></IconButton>
      </div>
    </div>
  )
}

/** Senza contatti (e sempre su Mac): come funziona, in tre passi. */
function Guide() {
  const steps: Array<[Tone, LucideIcon, string]> = [
    ["sky", AppWindowIcon, "Apri questo programma"],
    ["primary", KeyRoundIcon, "Comunica ID e password al tecnico"],
    ["emerald", CheckIcon, "Accetta la richiesta di collegamento"],
  ]
  return (
    <section className="space-y-1.5">
      <div className="text-muted-foreground px-1 text-[11px] font-medium tracking-wide uppercase">Come funziona</div>
      <ol className="bg-muted/40 divide-border/60 divide-y rounded-xl">
        {steps.map(([tone, Icon, text], i) => (
          <li key={text} className="flex items-center gap-3 px-3 py-2">
            <span className={cn("grid size-7 shrink-0 place-items-center rounded-md", TONE[tone])}><Icon className="size-4" /></span>
            <span className="min-w-0 flex-1 text-xs">{text}</span>
            <span className="text-muted-foreground/70 font-mono text-[11px] font-semibold tabular-nums">{i + 1}</span>
          </li>
        ))}
      </ol>
    </section>
  )
}

/** Contatti compatti (2×2): un clic copia il valore. */
function Contacts() {
  return (
    <section className="space-y-1.5">
      <div className="text-muted-foreground px-1 text-[11px] font-medium tracking-wide uppercase">Contatti dell'assistenza</div>
      <div className="grid grid-cols-2 gap-1.5">
        {brand.contacts.map((c, i) => { const k = CONTACT[c.kind] ?? CONTACT.note; return (
          <Tooltip key={i}>
            <TooltipTrigger asChild>
              <button type="button" onClick={() => copyText(c.value, k.label)}
                className="bg-muted/40 hover:bg-muted focus-visible:ring-ring/50 flex min-w-0 cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-left outline-none focus-visible:ring-2">
                <span className={cn("grid size-6 shrink-0 place-items-center rounded-md", TONE[k.tone])}><k.icon className="size-3.5" /></span>
                <span className="min-w-0 truncate text-xs select-text">{c.value}</span>
              </button>
            </TooltipTrigger>
            <TooltipContent>{k.label} · clic per copiare</TooltipContent>
          </Tooltip>
        )})}
      </div>
    </section>
  )
}

function UnattendedDialog({ open, onOpenChange, permanentSet, onSaved, install, installing, onInstall, onUninstall }: {
  open: boolean; onOpenChange: (o: boolean) => void; permanentSet: boolean; onSaved: () => void
  install: InstallState; installing: boolean; onInstall: () => void; onUninstall: () => void
}) {
  const [pw, setPw] = React.useState("")
  const [pw2, setPw2] = React.useState("")
  const [confirmRemove, setConfirmRemove] = React.useState(false)
  React.useEffect(() => { if (open) { setPw(""); setPw2(""); setConfirmRemove(false) } }, [open])
  const installed = install === "installed"
  const tooShort = pw.length > 0 && pw.length < 8
  const mismatch = pw2.length > 0 && pw !== pw2
  const valid = pw.length >= 8 && pw === pw2
  const save = async (value: string) => {
    const ok = await invoke<boolean>("rd_set_permanent_password", { password: value }).catch(() => false)
    if (!ok) { toast.error("Password non salvata"); return }
    onOpenChange(false)
    onSaved()
    toast.success(value ? "Password permanente impostata" : "Password permanente tolta")
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-5 sm:max-w-sm">
        <DialogHeader className="items-center gap-3 text-center">
          <Tiles tiles={[["emerald", ShieldCheckIcon], ["primary", KeyRoundIcon], ["sky", PowerIcon]]} />
          <DialogTitle>Accesso non presidiato</DialogTitle>
          <DialogDescription>Con una password permanente il tecnico può collegarsi anche quando non sei al computer.</DialogDescription>
        </DialogHeader>
        <FieldGroup className="bg-muted/50 gap-4 rounded-xl p-3">
          <Field orientation="horizontal" data-disabled={install === "unsupported" || installing || undefined}>
            <FieldContent>
              <FieldLabel htmlFor="ua" className="font-normal">Avvia con il computer</FieldLabel>
              <FieldDescription>
                {install === "unsupported" ? "Per ora solo su Windows."
                  : installing ? "Conferma la richiesta di Windows: poi il client si riapre installato."
                  : confirmRemove ? "Il client viene disinstallato e si chiude."
                  : installed ? "Installato: si avvia con Windows, anche prima dell'accesso."
                  : "Installa il client: si avvia con Windows, anche prima dell'accesso."}
              </FieldDescription>
            </FieldContent>
            {installing ? <Spinner /> : confirmRemove ? (
              <div className="flex shrink-0 gap-1">
                <Button size="sm" variant="ghost" onClick={() => setConfirmRemove(false)}>Annulla</Button>
                <Button size="sm" variant="destructive" onClick={onUninstall}>Disinstalla</Button>
              </div>
            ) : (
              <Switch id="ua" disabled={install === "unsupported"} checked={installed}
                onCheckedChange={on => (on ? onInstall() : setConfirmRemove(true))} />
            )}
          </Field>
          <Field data-invalid={tooShort || undefined}>
            <FieldLabel htmlFor="ua-pw">{permanentSet ? "Nuova password permanente" : "Password permanente"}</FieldLabel>
            <Input id="ua-pw" type="password" value={pw} onChange={e => setPw(e.target.value)} placeholder="Almeno 8 caratteri"
              aria-invalid={tooShort || undefined} className="bg-background" />
            {tooShort && <FieldDescription>Almeno 8 caratteri.</FieldDescription>}
          </Field>
          <Field data-invalid={mismatch || undefined}>
            <FieldLabel htmlFor="ua-pw2">Ripeti la password</FieldLabel>
            <Input id="ua-pw2" type="password" value={pw2} onChange={e => setPw2(e.target.value)}
              aria-invalid={mismatch || undefined} className="bg-background" />
            {mismatch && <FieldDescription>Le due password non coincidono.</FieldDescription>}
          </Field>
        </FieldGroup>
        <DialogFooter className="gap-2">
          {permanentSet && <Button variant="ghost" className="text-destructive mr-auto" onClick={() => void save("")}>Togli</Button>}
          <Button variant="secondary" onClick={() => onOpenChange(false)}>Annulla</Button>
          <Button disabled={!valid} onClick={() => void save(pw)}><KeyRoundIcon /> Salva</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ── Finestra della connessione in arrivo (--cm) ────────────────────────────

interface Client {
  id: number; authorized: boolean; disconnected: boolean; name: string; peer_id: string
  keyboard: boolean; clipboard: boolean; file: boolean
}
interface ChatLine { from: "me" | "tech"; text: string; at: number }

/** Connessioni in arrivo dal nucleo: recupera gli eventi già arrivati (la
 *  pagina può caricarsi dopo) e scarta quelli già visti (numerati). */
function useConnections() {
  const [clients, setClients] = React.useState<Client[]>([])
  const [since, setSince] = React.useState<Record<number, number>>({})
  const [chat, setChat] = React.useState<Record<number, ChatLine[]>>({})
  const lastSeq = React.useRef(0)
  const seen = React.useRef(false)

  React.useEffect(() => {
    const apply = (ev: Record<string, unknown>) => {
      const seq = Number(ev.seq ?? 0)
      if (seq && seq <= lastSeq.current) return
      if (seq) lastSeq.current = seq
      if (ev.ev === "add") {
        const c = ev.client as Client
        seen.current = true
        setClients(cs => [...cs.filter(x => x.id !== c.id), c])
        if (c.authorized) setSince(m => (m[c.id] ? m : { ...m, [c.id]: Date.now() }))
      } else if (ev.ev === "remove") {
        setClients(cs => cs.filter(x => x.id !== ev.id))
      } else if (ev.ev === "chat") {
        const id = ev.id as number
        setChat(m => ({ ...m, [id]: [...(m[id] ?? []), { from: "tech", text: String(ev.text), at: Date.now() }] }))
      }
    }
    let alive = true
    const un = listen<Record<string, unknown>>("cm", e => apply(e.payload))
    void un.then(() => invoke<Array<Record<string, unknown>>>("cm_backlog")).then(list => { if (alive) list.forEach(apply) })
    return () => { alive = false; void un.then(f => f()) }
  }, [])

  // Una richiesta in attesa (anche quella dei soli file) ha la precedenza sulla sessione in corso.
  const client = clients.find(c => !c.authorized) ?? clients[0]
  const actions = {
    accept: (c: Client) => {
      void invoke("cm_authorize", { id: c.id })
      setSince(m => ({ ...m, [c.id]: Date.now() }))
      setClients(cs => cs.map(x => (x.id === c.id ? { ...x, authorized: true } : x)))
    },
    close: (c: Client) => void invoke("cm_close", { id: c.id }),
    send: (c: Client, text: string) => {
      void invoke("cm_chat", { id: c.id, text })
      setChat(m => ({ ...m, [c.id]: [...(m[c.id] ?? []), { from: "me", text, at: Date.now() }] }))
    },
  }
  return { client, clients, since, chat, seen, actions }
}

/** Finestra --cm (il client non era aperto): solo la connessione. */
function ConnectionWindow() {
  const { client, clients, since, chat, seen, actions } = useConnections()
  // Nessuna connessione rimasta: la finestra si chiude.
  React.useEffect(() => {
    if (seen.current && clients.length === 0) void getCurrentWindow().close()
  }, [clients.length, seen])
  return (
    <div className="bg-background text-foreground flex h-svh flex-col">
      <Header status={1} />
      <main className="min-h-0 flex-1 overflow-hidden">
        {!client ? (
          <div className="flex h-full items-center justify-center">
            <Hero tiles={[["sky", MonitorIcon], ["primary", HeadsetIcon], ["emerald", ShieldCheckIcon]]} title="In attesa…" />
          </div>
        ) : !client.authorized ? (
          <Incoming client={client} onAccept={() => actions.accept(client)} onReject={() => actions.close(client)} />
        ) : (
          <Session client={client} since={since[client.id] ?? Date.now()} chat={chat[client.id] ?? []}
            onSend={text => actions.send(client, text)} onEnd={() => actions.close(client)} />
        )}
      </main>
    </div>
  )
}

const PERMS = [
  ["keyboard", "Controllo di mouse e tastiera", MousePointer2Icon],
  ["clipboard", "Appunti condivisi", ClipboardIcon],
  ["file", "Trasferimento di file", FolderIcon],
] as const

function techName(c: Client) {
  return c.name ? `${c.name}${c.peer_id ? ` · ${c.peer_id}` : ""}` : c.peer_id
}

/** Richiesta di collegamento: pannello che sale dal fondo della finestra principale. */
function RequestPanel({ client, onAccept, onReject }: { client: Client; onAccept: () => void; onReject: () => void }) {
  return (
    <div role="dialog" aria-label="Richiesta di assistenza"
      className="bg-background animate-in slide-in-from-bottom fade-in absolute inset-x-0 bottom-0 z-40 flex flex-col gap-3 rounded-t-2xl border-t px-5 pt-3 pb-4 shadow-[0_-12px_32px_-12px_rgb(0_0_0/0.25)] duration-300">
      <div className="bg-muted mx-auto h-1 w-10 rounded-full" />
      <div className="flex items-center gap-3">
        <span className="relative grid size-11 shrink-0 place-items-center">
          <span className="bg-primary/20 absolute inset-0 animate-ping rounded-xl [animation-duration:2s]" />
          <span className={cn("relative grid size-11 place-items-center rounded-xl", TONE.primary)}><HeadsetIcon className="size-5" /></span>
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold">Richiesta di assistenza</div>
          <div className="text-muted-foreground truncate text-xs"><b className="text-foreground font-medium">{techName(client)}</b> chiede di collegarsi</div>
        </div>
      </div>
      <div className="bg-muted/50 grid gap-2 rounded-xl p-2.5">
        {PERMS.map(([k, label, Icon]) => (
          <Field key={k} orientation="horizontal">
            <Icon className="text-muted-foreground size-4 shrink-0" />
            <FieldContent><FieldLabel htmlFor={`rp-${k}`} className="text-xs font-normal">{label}</FieldLabel></FieldContent>
            <Switch id={`rp-${k}`} defaultChecked={client[k]} onCheckedChange={v => void invoke("cm_permission", { id: client.id, name: k, enabled: v })} />
          </Field>
        ))}
      </div>
      <div className="flex gap-2">
        <Button variant="secondary" className="flex-1" onClick={onReject}><XIcon /> Rifiuta</Button>
        <Button className="flex-1" onClick={onAccept}><CheckIcon /> Accetta</Button>
      </div>
    </div>
  )
}

function Incoming({ client, onAccept, onReject }: { client: Client; onAccept: () => void; onReject: () => void }) {
  return (
    <div className="flex h-full flex-col justify-center gap-5 px-5 py-4">
      <div className="flex flex-col items-center gap-3 text-center">
        <Tiles tiles={[["sky", MonitorIcon], ["primary", SparklesIcon], ["emerald", HeadsetIcon]]} />
        <div className="text-base font-semibold">Richiesta di assistenza</div>
        <p className="text-muted-foreground text-xs"><b className="text-foreground font-medium">{techName(client)}</b> chiede di collegarsi a questo computer.</p>
      </div>
      <FieldGroup className="bg-muted/50 gap-3 rounded-xl p-3">
        {PERMS.map(([k, label, Icon]) => (
          <Field key={k} orientation="horizontal">
            <Icon className="text-muted-foreground size-4 shrink-0" />
            <FieldContent><FieldLabel htmlFor={`p-${k}`} className="font-normal">{label}</FieldLabel></FieldContent>
            <Switch id={`p-${k}`} defaultChecked={client[k]} onCheckedChange={v => void invoke("cm_permission", { id: client.id, name: k, enabled: v })} />
          </Field>
        ))}
      </FieldGroup>
      <div className="flex gap-2">
        <Button variant="secondary" className="flex-1" onClick={onReject}><XIcon /> Rifiuta</Button>
        <Button className="flex-1" onClick={onAccept}><CheckIcon /> Accetta</Button>
      </div>
    </div>
  )
}

function Session({ client, since, chat, onSend, onEnd, compact }: { client: Client; since: number; chat: ChatLine[]; onSend: (t: string) => void; onEnd: () => void; compact?: boolean }) {
  const [now, setNow] = React.useState(Date.now())
  React.useEffect(() => { const t = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(t) }, [])
  const secs = Math.max(0, Math.floor((now - since) / 1000))
  const time = `${String(Math.floor(secs / 60)).padStart(2, "0")}:${String(secs % 60).padStart(2, "0")}`
  const [draft, setDraft] = React.useState("")
  const end = React.useRef<HTMLDivElement>(null)
  React.useEffect(() => { end.current?.scrollIntoView({ block: "nearest" }) }, [chat.length])
  return (
    <div className={cn("flex flex-col gap-3", compact ? "min-h-0 flex-1" : "h-full px-5 py-4")}>
      <div className="flex items-center gap-3">
        <span className="relative flex size-3">
          <span className="bg-primary absolute inline-flex size-full animate-ping rounded-full opacity-60" />
          <span className="bg-primary relative inline-flex size-3 rounded-full" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold">Sessione in corso</div>
          <div className="text-muted-foreground truncate text-xs">{techName(client)} vede e controlla questo computer</div>
        </div>
        <span className="text-muted-foreground font-mono text-sm tabular-nums">{time}</span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {PERMS.filter(([k]) => client[k]).map(([k, label, Icon]) => (
          <span key={k} className="bg-muted text-muted-foreground inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-[11px]"><Icon className="size-3" /> {label}</span>
        ))}
      </div>
      <ScrollArea className="bg-muted/40 min-h-0 flex-1 rounded-xl">
        <div className="flex flex-col gap-2 p-3">
          {chat.length === 0 && (
            <div className="flex flex-col items-center gap-3 py-6 text-center">
              <Tiles tiles={[["sky", MessageSquareIcon], ["primary", HeadsetIcon], ["amber", SparklesIcon]]} />
              <p className="text-muted-foreground text-xs">Scrivi qui se vuoi dire qualcosa al tecnico.</p>
            </div>
          )}
          {chat.map((m, i) => (
            <div key={i} className={cn("max-w-[85%] rounded-2xl px-3 py-1.5 text-sm break-words",
              m.from === "me" ? "bg-primary text-primary-foreground self-end rounded-br-md" : "bg-background self-start rounded-bl-md")}>{m.text}</div>
          ))}
          <div ref={end} />
        </div>
      </ScrollArea>
      <form className="flex gap-2" onSubmit={e => { e.preventDefault(); if (draft.trim()) { onSend(draft.trim()); setDraft("") } }}>
        <Input value={draft} onChange={e => setDraft(e.target.value)} placeholder="Scrivi un messaggio…" />
        <Button type="submit" size="icon" aria-label="Invia" disabled={!draft.trim()}><SendIcon /></Button>
      </form>
      <Button variant="destructive" onClick={onEnd}><PowerIcon /> Termina la sessione</Button>
    </div>
  )
}

/** Tema: automatico (sistema), chiaro o scuro. */
function ThemeSwitch({ className }: { className?: string }) {
  const [t, setT] = React.useState<ThemeChoice>(getTheme)
  const opts: Array<[ThemeChoice, LucideIcon, string]> = [["system", LaptopIcon, "Come il sistema"], ["light", SunIcon, "Chiaro"], ["dark", MoonIcon, "Scuro"]]
  return (
    <div className={cn("bg-muted/60 flex items-center rounded-md p-0.5", className)}>
      {opts.map(([v, Icon, label]) => (
        <Tooltip key={v}>
          <TooltipTrigger asChild>
            <button type="button" aria-label={label} aria-pressed={t === v} onClick={() => { setTheme(v); setT(v) }}
              className={cn("text-muted-foreground grid size-6 place-items-center rounded-[5px] outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                t === v && "bg-background text-foreground shadow-xs")}>
              <Icon className="size-3.5" />
            </button>
          </TooltipTrigger>
          <TooltipContent>{label}</TooltipContent>
        </Tooltip>
      ))}
    </div>
  )
}

function IconButton({ label, onClick, children, disabled }: { label: string; onClick: () => void; children: React.ReactNode; disabled?: boolean }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" aria-label={label} disabled={disabled} onClick={onClick} className="text-muted-foreground hover:bg-background shrink-0">{children}</Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
