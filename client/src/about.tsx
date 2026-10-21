import * as React from "react"
import { HeartIcon, SearchIcon } from "lucide-react"

import { brand } from "@/lib/brand"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"

// Informazioni e licenze: crediti a RustDesk, licenza AGPL e sorgente, e
// l'elenco delle librerie open source incluse (generato da
// scripts/gen-licenses.cjs, caricato solo quando si apre).

interface Lib { name: string; version: string; license: string; url: string }

const SOURCE = "https://git.difonzo.net/andrea/merlin-rustdesk"

export function AboutDialog({ open, onOpenChange, version }: { open: boolean; onOpenChange: (o: boolean) => void; version: string }) {
  const [libs, setLibs] = React.useState<{ rust: Lib[]; npm: Lib[] } | null>(null)
  const [q, setQ] = React.useState("")
  React.useEffect(() => {
    if (open && !libs) void import("@/assets/licenses.json").then(m => setLibs(m.default as { rust: Lib[]; npm: Lib[] }))
  }, [open, libs])
  const filter = (l: Lib[]) => { const s = q.trim().toLowerCase(); return s ? l.filter(x => x.name.toLowerCase().includes(s) || x.license.toLowerCase().includes(s)) : l }
  const rust = filter(libs?.rust ?? []), npm = filter(libs?.npm ?? [])
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[90svh] flex-col gap-4 sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{brand.name}</DialogTitle>
          <DialogDescription>Versione {version || "—"}</DialogDescription>
        </DialogHeader>
        <section className="bg-muted/50 space-y-2 rounded-xl p-3 text-xs leading-relaxed">
          <p className="flex items-center gap-1.5 font-medium"><HeartIcon className="text-rose-500 size-3.5" /> Basato su RustDesk</p>
          <p>
            Questo programma usa il nucleo di <span className="font-medium">RustDesk</span> (rustdesk.com), software libero di
            Purslane Ltd. e dei suoi collaboratori, rilasciato con licenza <span className="font-medium">GNU AGPL-3.0</span>.
            Grazie a tutto il progetto RustDesk.
          </p>
          <p>
            Anche questo client è distribuito con licenza AGPL-3.0, senza alcuna garanzia. Il codice sorgente completo, con le modifiche,
            è disponibile qui: <span className="font-mono select-text">{SOURCE}</span>
          </p>
        </section>
        <div className="flex min-h-0 flex-1 flex-col gap-2">
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground text-[11px] font-medium tracking-wide uppercase">Librerie open source</span>
            <span className="text-muted-foreground ml-auto text-[11px] tabular-nums">{libs ? libs.rust.length + libs.npm.length : ""}</span>
          </div>
          <div className="relative">
            <SearchIcon className="text-muted-foreground pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2" />
            <Input value={q} onChange={e => setQ(e.target.value)} placeholder="Cerca per nome o licenza" className="h-8 pl-7 text-xs" />
          </div>
          <ScrollArea className="h-64 rounded-lg border">
            <div className="p-2 text-xs">
              {!libs && <p className="text-muted-foreground p-2">Caricamento…</p>}
              {[["Rust", rust], ["Interfaccia (npm)", npm]].map(([title, list]) => (list as Lib[]).length > 0 && (
                <div key={title as string} className="mb-2">
                  <div className="text-muted-foreground px-1 py-1 font-medium">{title as string}</div>
                  {(list as Lib[]).map(l => (
                    <div key={`${l.name}@${l.version}`} className="flex gap-2 rounded px-1 py-0.5 hover:bg-muted/50" title={l.url}>
                      <span className="min-w-0 flex-1 truncate select-text">{l.name} <span className="text-muted-foreground">{l.version}</span></span>
                      <span className="text-muted-foreground shrink-0">{l.license}</span>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </ScrollArea>
        </div>
      </DialogContent>
    </Dialog>
  )
}
