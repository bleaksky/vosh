import type { UiConfig } from '../../../../ipc/uiConfig';
import type { UpdateConfig } from '../../legacy/useSettingsAutoSave';
import { Row, Segmented, type SegmentedOption } from '../../../../ui';

// In a fight and Attack lines, the two rows under Collapse repeated
// lines on Appearance, Terminal text. In a fight says whether the lines
// that arrive while the game names your opponent collapse, and starts on
// Collapse. Attack lines says whether each hit and miss does, and starts
// on Show every line, so a count never hides how many hits landed. The
// session shows every attack line while In a fight shows every line, so
// Attack lines waits then and says why. Both rows show while Collapse
// repeated lines is on, and a link to either shows them while it is off,
// waiting, so search lands on them.

type CollapseChoice = 'collapse' | 'every';

const CHOICES: readonly SegmentedOption<CollapseChoice>[] = [
  { value: 'collapse', label: 'Collapse' },
  { value: 'every', label: 'Show every line' },
];

const WAITING = CHOICES.map((choice) => ({ ...choice, disabled: true }));

/** What a row says while Collapse repeated lines is off. */
const OFF = 'Turn on Collapse repeated lines to choose.';

const FIGHTS = 'Every line that arrives while you are fighting.';
const ATTACKS = 'Each hit and miss the game shows you, in a fight or not.';
const ATTACKS_WAIT = 'Attack lines show every line while In a fight does.';

const choiceOf = (on: boolean): CollapseChoice => (on ? 'collapse' : 'every');

interface CollapseRowsProps {
  config: UiConfig;
  update: UpdateConfig;
}

export function CollapseRows({ config, update }: CollapseRowsProps) {
  const on = config.collapse_repeats;
  // Attack lines waits unless the lines of a fight collapse.
  const attacksWait = !on || !config.collapse_fight_lines;
  return (
    <>
      <Row
        anchor="collapse-fights"
        label="In a fight"
        description={on ? FIGHTS : OFF}
        {...(on ? {} : { className: 'is-disabled' })}
      >
        <Segmented
          options={on ? CHOICES : WAITING}
          value={choiceOf(config.collapse_fight_lines)}
          onChange={(choice) =>
            update({ collapse_fight_lines: choice === 'collapse' }, { now: true })
          }
        />
      </Row>
      <Row
        anchor="collapse-attacks"
        label="Attack lines"
        description={!on ? OFF : attacksWait ? ATTACKS_WAIT : ATTACKS}
        {...(attacksWait ? { className: 'is-disabled' } : {})}
      >
        <Segmented
          options={attacksWait ? WAITING : CHOICES}
          // While In a fight shows every line, so do attack lines, and
          // the row says so while it waits on Collapse repeated lines too.
          value={config.collapse_fight_lines ? choiceOf(config.collapse_attack_lines) : 'every'}
          onChange={(choice) =>
            update({ collapse_attack_lines: choice === 'collapse' }, { now: true })
          }
        />
      </Row>
    </>
  );
}
