import { useMemo } from 'react';
import { affectThresholdsOf } from './affectsDisplay';
import { useAffectFull } from '../../stores/gmcp/affectFullStore';
import { useAffectsDisplay } from '../../stores/config/affectsDisplayStore';
import { useAffects, useAffectsHidden } from '../../stores/gmcp/affectsStore';
import { useTrackedAffects } from '../../stores/config/trackedAffectsStore';
import { ChipsView } from './AffectsChips';
import { CountdownView } from './AffectsCountdown';
import { AffectsPaneView } from './AffectsTimers';

// Timers first (AffectsTimers.tsx) is one of four styles you pick in
// Settings, Layout, Affects or the pane menu. Countdown
// (AffectsCountdown.tsx) lists every affect by the hours it has left,
// and Grouped chips (AffectsChips.tsx) puts what to recast first.
// Draining chips draws the same chips, and a chip running out colors
// only the share that matches the hours it has left.
//
// The hours at which an affect runs out and is almost gone come from
// there as well, two and one unless you change them, and every style
// colors the hours, the marks, the header counts and what to recast by
// them.

export function AffectsPane() {
  const current = useAffects();
  const tracked = useTrackedAffects();
  const hidden = useAffectsHidden();
  const display = useAffectsDisplay();
  const full = useAffectFull();
  // The store keeps one display while nothing in it moves, so the views
  // keep their rows.
  const thresholds = useMemo(() => affectThresholdsOf(display), [display]);
  if (display.style === 'chips' || display.style === 'chips_drain') {
    return (
      <ChipsView
        current={current}
        tracked={tracked}
        hidden={hidden}
        full={full}
        thresholds={thresholds}
        fill={display.style === 'chips_drain' ? 'drain' : 'tint'}
      />
    );
  }
  if (display.style === 'countdown') {
    return (
      <CountdownView
        current={current}
        tracked={tracked}
        hidden={hidden}
        marker={display.marker}
        tint={display.tint}
        full={full}
        thresholds={thresholds}
      />
    );
  }
  return (
    <AffectsPaneView
      current={current}
      tracked={tracked}
      hidden={hidden}
      marker={display.marker}
      tint={display.tint}
      thresholds={thresholds}
    />
  );
}
