import { useContext, type CSSProperties, type ReactNode } from 'react';
import { drawSample, quotedWords, sampleRunCss } from './presetSample';
import type { Preset } from './presets';
import { cx } from '../ui';
import { SamplePaintContext } from './samplePaint';

/** A preset's sample as the terminal draws it, one line each, each run
 *  in the color the preset paints it, or in `colors`, yours by key. The
 *  words a tell quotes draw as a bar in the color of their run. The
 *  Presets page and Get started both show it. */
export function PresetSample({
  preset,
  colors,
  className,
}: {
  preset: Preset;
  colors?: Readonly<Record<string, string>>;
  className?: string;
}) {
  const paint = useContext(SamplePaintContext);
  return (
    <div
      className={cx('st-auto-sample', className)}
      style={paint ? ({ '--sample-fg': paint.foreground } as CSSProperties) : undefined}
    >
      {preset.sample.map((line, n) => {
        const { runs } = drawSample(preset, line, colors);
        const bar = quotedWords(runs.map(([text]) => text).join(''));
        const parts: ReactNode[] = [];
        let at = 0;
        runs.forEach(([text, color], r) => {
          const css = paint ? sampleRunCss(color, paint) : { bold: false };
          const style = css.color ? ({ '--sample-fg': css.color } as CSSProperties) : undefined;
          const piece = (key: string, body: ReactNode) => (
            <span key={key} className={cx(css.bold && 'is-bold')} style={style}>
              {body}
            </span>
          );
          const start = at;
          at += text.length;
          if (!bar || bar[1] <= start || bar[0] >= at) {
            parts.push(piece(`${r}`, text));
            return;
          }
          const from = Math.max(bar[0], start) - start;
          const to = Math.min(bar[1], at) - start;
          parts.push(
            piece(
              `${r}`,
              <>
                {text.slice(0, from)}
                <span
                  className="st-auto-sample-bar"
                  style={{ width: `${to - from}ch` }}
                  role="img"
                  aria-label="Words"
                />
                {text.slice(to)}
              </>,
            ),
          );
        });
        return (
          <div key={n} className="st-auto-sample-line">
            {parts}
          </div>
        );
      })}
    </div>
  );
}
