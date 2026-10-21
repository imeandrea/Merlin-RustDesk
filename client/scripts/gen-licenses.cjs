#!/usr/bin/env node
// Elenco delle librerie open source incluse nel client (Rust e npm, senza
// quelle usate solo per lo sviluppo) → src/assets/licenses.json, mostrato in
// «Informazioni e licenze». Rilanciare dopo aver cambiato le dipendenze:
//
//   node scripts/gen-licenses.cjs
//
// Richiede cargo (cargo metadata) e il package-lock.json del client.

const fs = require('fs'), path = require('path'), cp = require('child_process')
const CLIENT = path.resolve(__dirname, '..')

function rust() {
  const meta = JSON.parse(cp.execSync('cargo metadata --format-version 1', { cwd: path.join(CLIENT, 'src-tauri'), maxBuffer: 1 << 28 }).toString())
  const byId = new Map(meta.packages.map(p => [p.id, p]))
  const nodes = new Map(meta.resolve.nodes.map(n => [n.id, n]))
  const workspace = new Set(meta.workspace_members)
  const root = meta.packages.find(p => p.name === 'merlin-rustdesk-client')
  // Dipendenze effettive del client (senza quelle di sviluppo o di build).
  const seen = new Set(), stack = [root.id]
  while (stack.length) {
    const id = stack.pop()
    if (seen.has(id)) continue
    seen.add(id)
    for (const d of nodes.get(id)?.deps ?? []) {
      if (d.dep_kinds.some(k => k.kind === null)) stack.push(d.pkg)
    }
  }
  return [...seen].filter(id => !workspace.has(id)).map(id => byId.get(id))
    .map(p => ({ name: p.name, version: p.version, license: p.license || 'vedi il pacchetto', url: p.repository || `https://crates.io/crates/${p.name}` }))
}

function npm() {
  const lock = JSON.parse(fs.readFileSync(path.join(CLIENT, 'package-lock.json'), 'utf8'))
  return Object.entries(lock.packages)
    .filter(([k, v]) => k.startsWith('node_modules/') && !v.dev && !v.optional)
    .map(([k, v]) => {
      const name = k.slice(k.lastIndexOf('node_modules/') + 'node_modules/'.length)
      let license = v.license, url = `https://www.npmjs.com/package/${name}`
      try {
        const pj = JSON.parse(fs.readFileSync(path.join(CLIENT, k, 'package.json'), 'utf8'))
        license = license || (typeof pj.license === 'string' ? pj.license : pj.license?.type)
        const repo = typeof pj.repository === 'string' ? pj.repository : pj.repository?.url
        if (repo) url = repo.replace(/^git\+/, '').replace(/\.git$/, '').replace(/^git:\/\//, 'https://')
      } catch { /* pacchetto non installato: restano i dati del lock */ }
      return { name, version: v.version, license: license || 'vedi il pacchetto', url }
    })
}

const uniq = list => [...new Map(list.map(x => [`${x.name}@${x.version}`, x])).values()].sort((a, b) => a.name.localeCompare(b.name))
const out = { rust: uniq(rust()), npm: uniq(npm()) }
fs.writeFileSync(path.join(CLIENT, 'src/assets/licenses.json'), JSON.stringify(out))
console.log(`licenze: ${out.rust.length} crate Rust, ${out.npm.length} pacchetti npm`)
