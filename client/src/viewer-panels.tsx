// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

import * as React from "react"
import { Channel, invoke } from "@tauri-apps/api/core"
import { getCurrentWebview } from "@tauri-apps/api/webview"
import { open } from "@tauri-apps/plugin-dialog"
import {
  ArrowUpIcon, DownloadIcon, FileIcon, FolderIcon, KeyRoundIcon, RefreshCwIcon, SendIcon, UploadIcon, XIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

// Pannelli a destra del visore: chat con chi sta al PC remoto e file del PC
// remoto (seconda connessione di tipo «file», comandi con `viewer_files`).

export interface ChatLine { from: "me" | "peer"; text: string }

/** Intestazione dei pannelli: alta come la barra del visore. */
function PanelHeader({ title, onClose, children }: { title: string; onClose: () => void; children?: React.ReactNode }) {
  return (
    <div className="flex h-10 shrink-0 items-center gap-1 border-b pr-1.5 pl-3">
      <span className="min-w-0 flex-1 truncate text-sm font-medium">{title}</span>
      {children}
      <Button size="icon" variant="ghost" className="size-7" aria-label="Chiudi il pannello" onClick={onClose}><XIcon /></Button>
    </div>
  )
}

export function ChatPanel({ lines, onSend, onClose }: { lines: ChatLine[]; onSend: (text: string) => void; onClose: () => void }) {
  const [draft, setDraft] = React.useState("")
  const end = React.useRef<HTMLDivElement>(null)
  React.useEffect(() => { end.current?.scrollIntoView({ block: "nearest" }) }, [lines.length])
  const send = () => {
    const text = draft.trim()
    if (!text) return
    onSend(text)
    setDraft("")
  }
  return (
    <>
      <PanelHeader title="Chat" onClose={onClose} />
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-1.5 p-3 text-sm select-text">
          {lines.length === 0 && <p className="text-muted-foreground text-xs">Nessun messaggio.</p>}
          {lines.map((m, i) => (
            <div key={i} className={cn("max-w-[85%] rounded-lg px-2.5 py-1.5", m.from === "me" ? "bg-primary text-primary-foreground self-end" : "bg-muted self-start")}>{m.text}</div>
          ))}
          <div ref={end} />
        </div>
      </ScrollArea>
      <form className="flex shrink-0 gap-1.5 border-t p-2" onSubmit={e => { e.preventDefault(); send() }}>
        <Input value={draft} onChange={e => setDraft(e.target.value)} placeholder="Scrivi un messaggio…" />
        <Button type="submit" size="icon" disabled={!draft.trim()} aria-label="Invia"><SendIcon /></Button>
      </form>
    </>
  )
}

interface Entry { name: string; dir: boolean; size: number; time: number; hidden: boolean }
interface Transfer { job: number; name: string; download: boolean; total: number; done: number; state: "run" | "ok" | "error"; error?: string }

const files = (v: Record<string, unknown>) => { void invoke("viewer_files", { input: v }).catch(() => {}) }

const sizeText = (n: number) => (n < 1024 ? `${n} B` : n < 1024 ** 2 ? `${(n / 1024).toFixed(0)} KB` : n < 1024 ** 3 ? `${(n / 1024 ** 2).toFixed(1)} MB` : `${(n / 1024 ** 3).toFixed(1)} GB`)

/** Cartella che contiene `path` (separatore Windows se il PC remoto è Windows). */
function parentOf(path: string, windows: boolean): string {
  const trimmed = path.replace(/[\\/]+$/, "")
  const i = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"))
  if (i < 0) return windows ? "" : "/"
  const up = trimmed.slice(0, i)
  return up === "" ? "/" : windows && /^[A-Za-z]:$/.test(up) ? `${up}\\` : up
}
const joinRemote = (dir: string, name: string, windows: boolean) => {
  const sep = windows ? "\\" : "/"
  const d = dir.replace(/[\\/]+$/, "")
  return !windows && d === "" ? `/${name}` : `${d}${sep}${name}`
}

let nextJob = 1

/** File del PC remoto: cartelle, scarica nella cartella Download, carica (pulsante o trascinando). */
export function FilesPanel({ windows, onClose }: { windows: boolean; onClose: () => void }) {
  const [state, setState] = React.useState<"connecting" | "ready" | "ended">("connecting")
  const [needPassword, setNeedPassword] = React.useState<{ wrong: boolean } | null>(null)
  const [password, setPassword] = React.useState("")
  const [path, setPath] = React.useState("")
  const [entries, setEntries] = React.useState<Entry[]>([])
  const [loading, setLoading] = React.useState(false)
  const [transfers, setTransfers] = React.useState<Transfer[]>([])
  const [dropping, setDropping] = React.useState(false)
  const [downloads, setDownloads] = React.useState("")
  const pathRef = React.useRef(""); pathRef.current = path

  const patch = (job: number, p: Partial<Transfer>) => setTransfers(ts => ts.map(t => (t.job === job ? { ...t, ...p } : t)))

  React.useEffect(() => {
    void invoke<string>("viewer_download_dir").then(setDownloads).catch(() => {})
    const ch = new Channel<Record<string, unknown>>()
    ch.onmessage = e => {
      switch (e.ev) {
        case "fs_dir":
          setState("ready"); setNeedPassword(null); setLoading(false)
          setPath(String(e.path ?? ""))
          setEntries(((e.entries as Entry[]) ?? []).filter(x => !x.hidden).sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name)))
          break
        case "fs_need_password": setNeedPassword({ wrong: e.wrong === true }); break
        case "fs_job": patch(Number(e.job), { total: Number(e.total ?? 0) }); break
        case "fs_progress": patch(Number(e.job), { done: Number(e.done ?? 0) }); break
        case "fs_done":
          setTransfers(ts => ts.map(t => (t.job === Number(e.job) ? { ...t, state: "ok", done: t.total } : t)))
          // Caricato qui: la cartella remota si aggiorna.
          files({ t: "list", path: pathRef.current })
          break
        case "fs_error": patch(Number(e.job), { state: "error", error: String(e.err ?? "") }); break
        case "fs_ended": setState("ended"); break
      }
    }
    void invoke("viewer_files_open", { onEvent: ch })
  }, [])

  const upload = React.useCallback((paths: string[]) => {
    for (const p of paths) {
      const job = nextJob++
      const name = p.split(/[\\/]/).pop() ?? p
      setTransfers(ts => [{ job, name, download: false, total: 0, done: 0, state: "run" }, ...ts])
      files({ t: "upload", job, path: p, to: pathRef.current, windows })
    }
  }, [windows])

  // File trascinati sulla finestra: caricati nella cartella remota aperta.
  React.useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent(e => {
      if (e.payload.type === "over" || e.payload.type === "enter") setDropping(true)
      else if (e.payload.type === "leave") setDropping(false)
      else if (e.payload.type === "drop") { setDropping(false); if (state === "ready") upload(e.payload.paths) }
    })
    return () => { void un.then(f => f()) }
  }, [state, upload])

  const go = (p: string) => { setLoading(true); files({ t: "list", path: p }) }
  const download = (en: Entry) => {
    if (!downloads) return
    const job = nextJob++
    setTransfers(ts => [{ job, name: en.name, download: true, total: en.dir ? 0 : en.size, done: 0, state: "run" }, ...ts])
    files({ t: "download", job, path: joinRemote(path, en.name, windows), to: downloads })
  }
  const pick = async () => {
    const chosen = await open({ multiple: true, directory: false, title: "File da caricare sul PC remoto" }).catch(() => null)
    if (chosen) upload(Array.isArray(chosen) ? chosen : [chosen])
  }

  return (
    <>
      <PanelHeader title="File del PC remoto" onClose={onClose}>
        <Button size="icon" variant="ghost" className="size-7" aria-label="Carica file" title="Carica file" disabled={state !== "ready"} onClick={() => void pick()}><UploadIcon /></Button>
      </PanelHeader>
      {state === "ready" && (
        <div className="flex h-9 shrink-0 items-center gap-1 border-b px-1.5">
          <Button size="icon" variant="ghost" className="size-7" aria-label="Cartella superiore" title="Cartella superiore" onClick={() => go(parentOf(path, windows))}><ArrowUpIcon /></Button>
          <span className="min-w-0 flex-1 truncate font-mono text-xs" title={path}>{path || "Dischi"}</span>
          <Button size="icon" variant="ghost" className="size-7" aria-label="Aggiorna" title="Aggiorna" onClick={() => go(path)}>{loading ? <Spinner /> : <RefreshCwIcon />}</Button>
        </div>
      )}
      <div className="relative min-h-0 flex-1">
        {state === "connecting" && (
          <div className="flex h-full flex-col items-center justify-center gap-3 p-4 text-center text-sm">
            {needPassword ? (
              <form className="flex w-full flex-col gap-2" onSubmit={e => { e.preventDefault(); if (password) { files({ t: "login", password }); setPassword(""); setNeedPassword(null) } }}>
                <span className="flex items-center justify-center gap-2 font-medium"><KeyRoundIcon className="size-4" />{needPassword.wrong ? "Password errata" : "Password per i file"}</span>
                <Input type="password" autoFocus value={password} onChange={e => setPassword(e.target.value)} placeholder="Password del PC remoto" aria-invalid={needPassword.wrong} />
                <Button type="submit" disabled={!password}>Apri i file</Button>
                <span className="text-muted-foreground text-xs">Oppure attendi che sul PC remoto accettino la richiesta.</span>
              </form>
            ) : (
              <><Spinner /><span className="text-muted-foreground">Apertura dei file…</span></>
            )}
          </div>
        )}
        {state === "ended" && <p className="text-muted-foreground p-4 text-center text-sm">Connessione dei file chiusa.</p>}
        {state === "ready" && (
          <ScrollArea className="h-full">
            <div className="flex flex-col py-1">
              {entries.length === 0 && <p className="text-muted-foreground px-3 py-2 text-xs">Cartella vuota.</p>}
              {entries.map(en => (
                <div key={en.name} className="group hover:bg-muted/60 flex h-8 items-center gap-2 px-3 text-sm"
                  onDoubleClick={() => en.dir && go(joinRemote(path, en.name, windows))}>
                  {en.dir ? <FolderIcon className="size-4 shrink-0 text-sky-500" /> : <FileIcon className="text-muted-foreground size-4 shrink-0" />}
                  <button type="button" className={cn("min-w-0 flex-1 truncate text-left", en.dir && "cursor-pointer")} title={en.name}
                    onClick={() => en.dir && go(joinRemote(path, en.name, windows))}>{en.name}</button>
                  {!en.dir && <span className="text-muted-foreground shrink-0 text-xs tabular-nums">{sizeText(en.size)}</span>}
                  <Button size="icon" variant="ghost" className="size-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100" aria-label={`Scarica ${en.name}`} title="Scarica nella cartella Download"
                    onClick={() => download(en)}><DownloadIcon /></Button>
                </div>
              ))}
            </div>
          </ScrollArea>
        )}
        {dropping && state === "ready" && (
          <div className="border-primary bg-primary/10 text-primary pointer-events-none absolute inset-2 flex items-center justify-center rounded-lg border-2 border-dashed text-sm font-medium">
            Rilascia per caricare qui
          </div>
        )}
      </div>
      {transfers.length > 0 && (
        <div className="flex max-h-40 shrink-0 flex-col gap-1.5 overflow-y-auto border-t p-2">
          {transfers.map(t => (
            <div key={t.job} className="flex flex-col gap-1 text-xs">
              <div className="flex items-center gap-1.5">
                {t.download ? <DownloadIcon className="size-3.5 shrink-0" /> : <UploadIcon className="size-3.5 shrink-0" />}
                <span className="min-w-0 flex-1 truncate">{t.name}</span>
                <span className={cn("shrink-0", t.state === "error" ? "text-destructive" : "text-muted-foreground")}>
                  {t.state === "ok" ? "Fatto" : t.state === "error" ? "Errore" : t.total ? `${Math.round((t.done / t.total) * 100)}%` : "…"}
                </span>
                {t.state === "run" && (
                  <Button size="icon" variant="ghost" className="size-5" aria-label="Annulla" onClick={() => { files({ t: "cancel", job: t.job }); patch(t.job, { state: "error", error: "Annullato" }) }}><XIcon /></Button>
                )}
              </div>
              {t.state === "run" && (
                <div className="bg-muted h-1 overflow-hidden rounded-full">
                  <div className="bg-primary h-full transition-all" style={{ width: `${t.total ? Math.min(100, (t.done / t.total) * 100) : 5}%` }} />
                </div>
              )}
              {t.state === "error" && t.error && <span className="text-destructive truncate" title={t.error}>{t.error}</span>}
            </div>
          ))}
        </div>
      )}
    </>
  )
}
