// The four alert tones. A trigger's Alert row, an alert preset and
// mud.alert each name one, and the main window plays it when the session
// rings the alert. Web Audio draws them the way it draws the tick in
// tickSound.ts, so Vosh ships no sound file.
//
// While the main window hides on macOS, Rust plays a system sound in the
// tone's place (system_sound in src-tauri/src/alert/mac.rs). Each tone
// here echoes the sound that stands in for it, Glass for Chime, Ping for
// Bell, Tink for Knock and Basso for Low, so an alert sounds alike
// whether the window shows or not.

type AudioWindow = typeof window & { webkitAudioContext?: typeof AudioContext };

/** The tones an alert can name, in order. */
export const ALERT_TONES = [
  { value: 'chime', label: 'Chime' },
  { value: 'bell', label: 'Bell' },
  { value: 'knock', label: 'Knock' },
  { value: 'low', label: 'Low' },
] as const;

type AlertTone = (typeof ALERT_TONES)[number]['value'];

/** One oscillator of a tone. */
interface Note {
  /** Seconds after the tone starts. */
  at: number;
  hz: number;
  wave: OscillatorType;
  /** The loudest it gets. The tick peaks at 0.18, and no tone rings
   *  louder than that. */
  peak: number;
  /** Seconds it takes to reach its peak. */
  rise: number;
  /** Seconds from its start until it has faded out. */
  fade: number;
}

const TONES: Record<AlertTone, readonly Note[]> = {
  // Glass. Two short high notes, the second a fourth above the first.
  chime: [
    { at: 0, hz: 1318.5, wave: 'sine', peak: 0.16, rise: 0.008, fade: 0.22 },
    { at: 0.1, hz: 1760, wave: 'sine', peak: 0.15, rise: 0.008, fade: 0.5 },
  ],
  // Ping. One note with the out of tune partial a struck bell has, which
  // dies first, and a longer fade.
  bell: [
    { at: 0, hz: 1046.5, wave: 'sine', peak: 0.14, rise: 0.005, fade: 1.1 },
    { at: 0, hz: 2888.3, wave: 'sine', peak: 0.04, rise: 0.005, fade: 0.35 },
  ],
  // Tink. Two short low taps, the second a step lower.
  knock: [
    { at: 0, hz: 246.9, wave: 'triangle', peak: 0.18, rise: 0.004, fade: 0.08 },
    { at: 0.13, hz: 220, wave: 'triangle', peak: 0.16, rise: 0.004, fade: 0.08 },
  ],
  // Basso. One low note that swells in softly.
  low: [{ at: 0, hz: 196, wave: 'sine', peak: 0.18, rise: 0.03, fade: 0.6 }],
};

/** Play the alert tone `tone`. mud.alert passes any name, and one Vosh
 *  does not know plays Chime, as the system sound falls back to Glass.
 *  Does nothing where the web view has no audio. */
export function playAlertTone(tone: string): void {
  const notes = Object.hasOwn(TONES, tone) ? TONES[tone as AlertTone] : TONES.chime;
  try {
    const w = window as AudioWindow;
    const Ctx = w.AudioContext ?? w.webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const at = ctx.currentTime;
    // The audio goes once the last note ends.
    let playing = notes.length;
    for (const note of notes) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.frequency.value = note.hz;
      osc.type = note.wave;
      osc.connect(gain);
      gain.connect(ctx.destination);
      const start = at + note.at;
      gain.gain.setValueAtTime(0.0001, start);
      gain.gain.exponentialRampToValueAtTime(note.peak, start + note.rise);
      gain.gain.exponentialRampToValueAtTime(0.0001, start + note.fade);
      osc.start(start);
      osc.stop(start + note.fade + 0.02);
      osc.onended = () => {
        playing -= 1;
        if (playing === 0) void ctx.close();
      };
    }
  } catch {
    // No audio. The alert still rings everywhere else.
  }
}
