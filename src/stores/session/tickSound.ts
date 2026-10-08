import { getSelected } from './sessionsStore';

// The tick sound. The session reports each tick on session://tick, and
// the report that lands the tick carries fired true, once per tick. With
// Play a sound on it also carries sound true, and the tick store plays
// this short soft tone. Web Audio draws it, so Vosh ships no sound file.
// Every session keeps its own count, but only the session in front, the
// selected one, plays its tick, so two sessions never ring over
// each other.
//
// The sound was switched off while the backend fired on its own clock,
// which rang ahead of or behind the real tick. The game's own tick now
// decides when it fires, so the sound follows the tick again.

type AudioWindow = typeof window & { webkitAudioContext?: typeof AudioContext };

/** Reports closer together than this ring once. The backend fires once
 *  per tick, and this guards the ear against a double report. */
const MIN_GAP_MS = 500;

let lastPlayed = Number.NEGATIVE_INFINITY;

/** Play the tick sound of `session`, a 0.2 second 880 Hz tone that
 *  fades in and out. Does nothing for a session behind, or where the web
 *  view has no audio. */
export function playTickSound(session: number): void {
  if (session !== getSelected()) return;
  const now = Date.now();
  if (now - lastPlayed < MIN_GAP_MS) return;
  lastPlayed = now;
  try {
    const w = window as AudioWindow;
    const Ctx = w.AudioContext ?? w.webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.frequency.value = 880;
    osc.type = 'sine';
    osc.connect(gain);
    gain.connect(ctx.destination);
    const at = ctx.currentTime;
    gain.gain.setValueAtTime(0.0001, at);
    gain.gain.exponentialRampToValueAtTime(0.18, at + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.18);
    osc.start(at);
    osc.stop(at + 0.2);
    osc.onended = () => void ctx.close();
  } catch {
    // No audio. The count and the command still run.
  }
}
