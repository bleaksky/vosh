import { parseAnsi, type AnsiChunk } from './ansi';
import { renderPromptTemplate, type PromptVars } from './promptTemplate';
import type { VitalValues } from './stores/vitalsStore';

// The live preview under the prompt template on Settings, Input. It
// draws the template the way the terminal would with your vitals full,
// so you see its colors and styles before a prompt arrives.

/** Erelei's vitals at full, for when Vosh has not heard yours yet. */
export const SAMPLE_VITALS: VitalValues = {
  hp: 1020,
  maxhp: 1020,
  mana: 800,
  maxmana: 800,
  move: 930,
  maxmove: 930,
};

/** The prompt vars the preview draws with: each vital at its max, from
 *  your live maxes when Vosh has them and the sample otherwise. The
 *  short names (`mn`, `mv`) the vitals template uses carry the same
 *  values. */
export function promptPreviewVars(vitals: VitalValues | null): PromptVars {
  const known = vitals !== null && vitals.maxhp > 0;
  const v = known ? vitals : SAMPLE_VITALS;
  const hp = String(v.maxhp);
  const mana = String(v.maxmana > 0 ? v.maxmana : SAMPLE_VITALS.maxmana);
  const move = String(v.maxmove > 0 ? v.maxmove : SAMPLE_VITALS.maxmove);
  return {
    hp,
    maxhp: hp,
    mana,
    maxmana: mana,
    mn: mana,
    maxmn: mana,
    move,
    maxmove: move,
    mv: move,
    maxmv: move,
  };
}

/** The template drawn as styled runs of text, ready for spans. */
export function promptPreviewChunks(template: string, vitals: VitalValues | null): AnsiChunk[] {
  return parseAnsi(renderPromptTemplate(template, promptPreviewVars(vitals))).filter(
    (chunk) => chunk.text.length > 0,
  );
}
