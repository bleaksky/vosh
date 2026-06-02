import { useTickState } from '../lib/useTickState';
import { useWorldTime, formatMudTime, mudTimeColor } from '../lib/useWorldTime';
import { useChipStyle } from '../lib/useChipStyle';

// The single tick + mud time chip. Rides on the input row's top
// border line, centered horizontally. Wears the same bordered pill
// the old centered statusbar tick + time chip used (1px border-strong,
// 3px radius, surface-deep background, bold tabular values, accent-
// tinted time). Style picker still controls caption / icon /
// value-only rendering on each half. When neither tick nor mud time
// has data, the chip collapses entirely so the border line reads
// clean.
//
// Replaces the old TickPanel / TimePanel / InlineTick / InlineMudTime
// embed system: tick and mud time now have exactly one render site
// rather than nine zones.
export function LineChip() {
  const { active: tickActive, tickSecs, warn } = useTickState();
  const worldTime = useWorldTime();
  const mudTime = formatMudTime(worldTime);
  const timeColor = mudTimeColor(worldTime);
  const style = useChipStyle();

  const tickHere = tickActive;
  const timeHere = mudTime !== null;
  if (!tickHere && !timeHere) return null;

  return (
    <span className="line-chip" aria-label="tick and MUD time">
      {tickHere && (
        <span className="line-chip-block">
          {style === 'caption_value' && <span className="line-chip-caption">tick</span>}
          {style === 'icon_value' && (
            <span className="line-chip-icon" aria-hidden="true">
              ⏱
            </span>
          )}
          <span className={`line-chip-value${warn ? ' is-warn' : ''}`}>{tickSecs}s</span>
        </span>
      )}
      {tickHere && timeHere && (
        <span className="line-chip-sep" aria-hidden="true">
          ·
        </span>
      )}
      {timeHere && (
        <span className="line-chip-block">
          {style === 'caption_value' && <span className="line-chip-caption">time</span>}
          {style === 'icon_value' && (
            <span className="line-chip-icon" aria-hidden="true">
              ☀
            </span>
          )}
          <span
            className="line-chip-value line-chip-time"
            style={timeColor ? { color: timeColor } : undefined}
          >
            {mudTime}
          </span>
        </span>
      )}
    </span>
  );
}
