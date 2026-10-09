import { useEffect, useMemo } from 'react';
import { onPresetsChanged } from '../../ipc/automation';
import { getUiConfig } from '../../ipc/uiConfig';
import { allPanes } from '../../panel/paneLayout';
import { usePanelLayout } from '../../panel/panelLayoutStore';
import { PROMPT_SHOW_LABELS, usePromptShow } from '../../prompt/showState';
import { createConfigStore } from '../../stores/config/configStore';
import { useTrackedAffects } from '../../stores/config/trackedAffectsStore';
import { useSelectedRow } from '../../stores/session/sessionsStore';
import { noteFacts } from './getStartedStore';
import type { GetStartedFacts } from './steps';

// What the steps of Get started read live, the summary's metas among
// them, so a pane you close later shows there as it is. Each
// fact that finishes a step marks it done as it lands.

/** The presets the active profile has on, as stored, which a switch in
 *  the card or a Save in Settings changes, or null until the first read
 *  lands, since an empty list means the defaults. */
const enabledPresets = createConfigStore<string[] | null>({
  initial: null,
  read: () => getUiConfig().then((cfg) => cfg.enabled_presets),
  follow: (cb) =>
    onPresetsChanged(() => {
      getUiConfig()
        .then((cfg) => cb(cfg.enabled_presets))
        .catch(() => undefined);
    }),
});

export function useGetStartedFacts(): GetStartedFacts {
  const character = useSelectedRow()?.character ?? null;
  const presets = enabledPresets.use();
  const layout = usePanelLayout();
  const tracked = useTrackedAffects().length;
  const show = usePromptShow()?.show;
  const facts = useMemo<GetStartedFacts>(
    () => ({
      character,
      enabledPresets: presets,
      panes: layout ? allPanes(layout.root) : [],
      tracked,
      promptPlace: show && show !== 'text' ? PROMPT_SHOW_LABELS[show] : null,
    }),
    [character, presets, layout, tracked, show],
  );
  useEffect(() => noteFacts(facts), [facts]);
  return facts;
}
