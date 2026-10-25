// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

// Piccolo suono per un messaggio in arrivo (chat di RustDesk): due note brevi
// generate con Web Audio, senza file audio. Niente notifiche a schermo.

let ctx: AudioContext | null = null

/** Suona il «ding»; false dove Web Audio non c'è. */
export function playChime(): boolean {
  const Ctx = globalThis.AudioContext ?? (globalThis as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
  if (!Ctx) return false
  try {
    ctx ??= new Ctx()
    void ctx.resume().catch(() => {})
    const t = ctx.currentTime
    for (const [i, freq] of [880, 1320].entries()) {
      const osc = ctx.createOscillator()
      const gain = ctx.createGain()
      const start = t + i * 0.09
      osc.type = "sine"
      osc.frequency.value = freq
      gain.gain.setValueAtTime(0.0001, start)
      gain.gain.exponentialRampToValueAtTime(0.08, start + 0.01)
      gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.18)
      osc.connect(gain).connect(ctx.destination)
      osc.start(start)
      osc.stop(start + 0.2)
    }
    return true
  } catch {
    return false
  }
}
