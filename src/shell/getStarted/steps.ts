import { enabledPresetIds } from '../../automation/automationRecords';
import { PRESETS, type Preset } from '../../automation/presets';
import type { ConnectionTarget } from '../../ipc/session';
import { knownWorld } from '../../lib/knownWorlds';

// The steps of Get started, board 2 of the First Run review. The list
// follows the world you connect to (Q6, Q7). The Forsaken Lands gets
// five steps, the last two under After you log in. Any other game gets
// two, since the presets and the Affects, Chat and Group panes wait on
// what only Aabahran sends.
//
// A step is done once its rule holds, and Get started keeps its id from
// then on. Connect is done when the game names you in Char.Status or
// sends your vitals, since a login to a character left link dead sends
// no Char.Status, or on another game at its first line. The presets,
// panes and affects steps are done when the facts below say so, and the
// prompt step when you press Done in the prompt card.

/** A step, by the id profiles.toml keeps in `done`. */
export type StepId = 'connect' | 'presets' | 'panes' | 'affects' | 'prompt';

export interface Step {
  id: StepId;
  title: string;
  /** What the step does, in a line. */
  line: string;
  /** The step waits for your first room, under After you log in. */
  afterLogin: boolean;
}

/** What the steps read live, for their done rules and the summary. */
export interface GetStartedFacts {
  /** The character the selected session plays, or null. */
  character: string | null;
  /** The presets the profile has on, as its file stores them, or null
   *  until Vosh reads them. */
  enabledPresets: readonly string[] | null;
  /** The panes the panel holds. */
  panes: readonly string[];
  /** How many affects the profile tracks. */
  tracked: number;
  /** Where your prompt shows, or null while it shows in the text or
   *  Vosh has not read it yet. */
  promptPlace: string | null;
}

/** Whether a target plays The Forsaken Lands, the one world the full
 *  list is made for. */
export function onForsakenLands(host: string): boolean {
  return knownWorld(host) !== undefined;
}

/** The presets the presets step suggests on a world, outside Chat,
 *  whose step lists its own. */
export function suggestedPresets(host: string): Preset[] {
  const world = knownWorld(host)?.name;
  if (!world) return [];
  return PRESETS.filter((p) => p.category !== 'chat' && p.suggest.includes(world));
}

/** The steps for the world you connect to, in the order the card lists
 *  them. */
export function stepsFor(target: ConnectionTarget): Step[] {
  const world = knownWorld(target.host);
  const name = world?.name ?? target.host.trim();
  const connect: Step = {
    id: 'connect',
    title: `Connect to ${name}`,
    line: `Vosh dials ${target.host.trim()} on port ${target.port}, and you log in at the game's own prompt.`,
    afterLogin: false,
  };
  const prompt: Step = {
    id: 'prompt',
    title: 'Customize your prompt',
    line: 'Lift your prompt onto a band, or pin it above the command line so it never scrolls away.',
    afterLogin: !!world,
  };
  if (!world) return [connect, prompt];
  return [
    connect,
    {
      id: 'presets',
      title: 'Color what the game prints',
      line: 'Presets color lines the game prints, so what matters stands out.',
      afterLogin: false,
    },
    {
      id: 'panes',
      title: 'Add Chat and Group',
      line: 'Chat gathers tells and channels in a pane of their own. Group shows everyone in your group with their health.',
      afterLogin: false,
    },
    {
      id: 'affects',
      title: 'Track the affects you keep up',
      line: 'Name the spells you keep up. Affects lists each one and marks it missing when it drops.',
      afterLogin: true,
    },
    prompt,
  ];
}

/** How many of the suggested presets are on. */
function suggestionsOn(host: string, facts: GetStartedFacts): number {
  if (!facts.enabledPresets) return 0;
  const on = new Set(enabledPresetIds(facts.enabledPresets));
  return suggestedPresets(host).filter((p) => on.has(p.id)).length;
}

/** The Chat and Group panes the panel holds, in that order. */
function socialPanes(facts: GetStartedFacts): string[] {
  return ['Chat', 'Group'].filter((pane) => facts.panes.includes(pane.toLowerCase()));
}

/** The steps the facts finish on a world: a suggestion on, a Chat or
 *  Group pane, and an affect tracked. Connect and the prompt finish on
 *  what happens, not on what holds. */
export function doneByFacts(host: string, facts: GetStartedFacts): StepId[] {
  if (!onForsakenLands(host)) return [];
  const done: StepId[] = [];
  if (suggestionsOn(host, facts) > 0) done.push('presets');
  if (socialPanes(facts).length > 0) done.push('panes');
  if (facts.tracked > 0) done.push('affects');
  return done;
}

/** What a step holds now, for its meta in the summary of board 5, or
 *  null while it holds nothing. */
export function stepMeta(id: StepId, host: string, facts: GetStartedFacts): string | null {
  switch (id) {
    case 'connect':
      return facts.character;
    case 'presets': {
      const on = suggestionsOn(host, facts);
      return on > 0 ? `${on} on` : null;
    }
    case 'panes': {
      const panes = socialPanes(facts);
      return panes.length > 0 ? panes.join(' and ') : null;
    }
    case 'affects':
      return facts.tracked > 0 ? `${facts.tracked} tracked` : null;
    case 'prompt':
      return facts.promptPlace;
  }
}

/** How many of `steps` are done. */
export function doneCount(steps: readonly Step[], done: readonly string[]): number {
  return steps.filter((step) => done.includes(step.id)).length;
}
