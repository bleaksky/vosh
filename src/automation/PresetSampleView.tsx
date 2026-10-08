import { useContext, type CSSProperties, type ReactNode } from 'react';
import { drawSample, sampleBars, sampleRunCss } from './presetSample';
import type { Preset } from './presets';
import { cx } from '../ui';
import { SamplePaintContext } from './samplePaint';

/** A preset's sample as the terminal draws it, one line each, each run
 *  in the color the preset paints it, or in `colors`, yours by key. The
 *  words a tell quotes and the line's own bars draw as bars in the color
 *  of their run. The
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
        const bars = sampleBars(runs.map(([text]) => text).join(''), line.bars);
        const parts: ReactNode[] = [];
        let at = 0;
        runs.forEach(([text, color], r) => {
          const css = paint ? sampleRunCss(color, paint) : { bold: false };
          const style = css.color ? ({ '--sample-fg': css.color } as CSSProperties) : undefined;
          const start = at;
          at += text.length;
          // The run as text, with a bar where one crosses it.
          const body: ReactNode[] = [];
          let from = 0;
          for (const [b0, b1] of bars) {
            if (b1 <= start || b0 >= at) continue;
            const s0 = Math.max(b0, start) - start;
            const s1 = Math.min(b1, at) - start;
            if (s0 > from) body.push(text.slice(from, s0));
            body.push(
              <span
                key={s0}
                className="st-auto-sample-bar"
                style={{ width: `${s1 - s0}ch` }}
                role="img"
                aria-label="Words"
              />,
            );
            from = s1;
          }
          if (from < text.length) body.push(text.slice(from));
          parts.push(
            <span key={r} className={cx(css.bold && 'is-bold')} style={style}>
              {body}
            </span>,
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
