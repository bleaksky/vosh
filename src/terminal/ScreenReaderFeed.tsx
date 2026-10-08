import { useEffect, useRef, useState } from 'react';
import {
  onScreenReader,
  type ScreenReaderFeed as Feed,
  type ScreenReaderOptions,
} from '../ipc/screenReader';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { useScreenReader } from '../stores/config/screenReaderStore';
import { useReader } from '../stores/session/readerStore';
import { getSelected, useSelected } from '../stores/session/sessionsStore';
import { announcement, PULSE_MS, setVoice } from './readerVoice';

// The game for a screen reader (R21 and R25 review, board 13, Q19 to
// Q21). While Read new game lines is on, the Terminal section holds a
// visually hidden log of the selected session's last 500 lines and a
// polite live region beside it. Both sit in the section and not in the
// terminal, so they read the same under the macOS underlay and under
// xterm.
//
// The log is role=log with aria-live off, so you can read back through
// it, and it never speaks by itself: a live log would read each line
// apart, and a pulse of the game reads as one announcement. The lines
// of the selected session that land within one pulse, 250 ms from the
// first or until a read brings your prompt, join one announcement. Past
// the burst you pick it reads how many came and the last of them. Read
// your prompt adds the prompt that ended the pulse. While no window of
// Vosh has focus it stays quiet unless Read in the background is on,
// and a session behind keeps its lines without a word.
//
// Each announcement is a fresh node in the live region, so the same
// words twice still speak. The prompt key, Mod+Shift+P, reads your
// latest prompt through the same region, through readPrompt in
// readerVoice.ts.

/** The lines and the prompt one pulse gathered. */
interface Pulse {
  lines: string[];
  count: number;
  prompt: string | null;
  timer: ReturnType<typeof setTimeout>;
}

export function ScreenReaderFeed() {
  const options = useScreenReader();
  return options.screen_reader ? <GameLines options={options} /> : null;
}

function GameLines({ options }: { options: ScreenReaderOptions }) {
  const reader = useReader();
  const selected = useSelected();
  const [said, setSaid] = useState<{ id: number; parts: string[] } | null>(null);
  const pulse = useRef<Pulse | null>(null);

  const say = (parts: string[]) => {
    if (parts.length > 0) setSaid((last) => ({ id: (last?.id ?? 0) + 1, parts }));
  };

  const end = () => {
    const now = pulse.current;
    if (!now) return;
    clearTimeout(now.timer);
    pulse.current = null;
    const prompt = options.screen_reader_prompt ? now.prompt : null;
    say(announcement(now.lines, now.count, prompt, options.screen_reader_burst));
  };
  const endRef = useRef(end);

  useTauriEvent(
    (cb: (heard: { feed: Feed; session: number }) => void) =>
      onScreenReader((feed, session) => cb({ feed, session })),
    ({ feed, session }) => {
      if (session !== getSelected()) return;
      if (feed.away && !options.screen_reader_background) return;
      const now = (pulse.current ??= {
        lines: [],
        count: 0,
        prompt: null,
        timer: setTimeout(() => endRef.current(), PULSE_MS),
      });
      // Only the newest lines up to the burst can be read, so a flood
      // within one pulse keeps no more than that.
      now.lines = [...now.lines, ...feed.lines].slice(-options.screen_reader_burst);
      now.count += feed.count;
      if (feed.prompt !== null) {
        now.prompt = feed.prompt;
        end();
      }
    },
  );

  // A pulse belongs to the session it heard, and goes when another
  // comes to the front or the reader turns off.
  useEffect(() => {
    return () => {
      if (pulse.current) clearTimeout(pulse.current.timer);
      pulse.current = null;
    };
  }, [selected]);

  useEffect(() => {
    endRef.current = end;
  });

  useEffect(() => setVoice(say), []);

  const first = reader.total - reader.lines.length;
  return (
    <>
      <ol className="visually-hidden" role="log" aria-live="off" aria-label="Game lines">
        {reader.lines.map((line, i) => (
          <li key={first + i}>{line}</li>
        ))}
      </ol>
      <div className="visually-hidden" aria-live="polite">
        {said && (
          <div key={said.id}>
            {said.parts.map((part, i) => (
              <p key={i}>{part}</p>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
