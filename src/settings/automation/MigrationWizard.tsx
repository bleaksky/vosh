import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { type TriggerPattern } from '../../ipc/automation';
import type { PresetEdit } from '../../ipc/presetEdits';
import { appQuit } from '../../ipc/windows';
import {
  migrationAnalyze,
  migrationApply,
  type MigrationConflict,
  type MigrationConflictResolution,
  type MigrationItemKind,
  type MigrationPlan,
} from '../../ipc/wizard';
import { patternSource } from '../../automation/automationTriggers';
import { changesLine } from '../../automation/presetEdits';
import { PRESETS, presetById } from '../../automation/presets';
import { presetChanges } from '../../automation/wizardPresets';
import { useEscape } from '../../lib/escapeStack';
import { listJoin } from '../../lib/text';
import { Button, Card, CardNote, Row, Section, Segmented, Select } from '../../ui';
import { DIALOG_FOCUSABLE, trapDialogFocus } from '../../ui/dialogFocus';

/** The id of every preset in the library this build installs from. */
const LIBRARY = PRESETS.map((p) => p.id);

/** A conflict with more versions than this picks from a select, since
 *  a segmented control that wide no longer fits the row. */
const MOST_SEGMENTS = 4;

/** Names the Merged as they are rows list before they count the rest. */
const MOST_NAMES = 4;

interface Props {
  onClose: () => void;
}

// The shared catalog preview, a 600 wide dialog over Settings built
// from the kit. It reads the plan, asks you to pick the version to keep
// of each item your profiles hold differently, and lists what merges as
// it is and the loadout each character gets. Apply copies each profile
// file to profiles/legacy, writes catalog.toml and loadouts.toml, and
// takes the aliases, triggers and macros out of each profile file,
// which keeps every other setting. The running app keeps its profiles
// until you open Vosh again, so the done state offers Quit Vosh.
export function MigrationWizard({ onClose }: Props) {
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Each conflict's chosen profile by conflictKey. The analyzer's
  // default seeds it, the one version that was on when exactly one was.
  const [picks, setPicks] = useState<Record<string, string>>({});
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const p = await migrationAnalyze(LIBRARY);
        if (cancelled) return;
        setPlan(p);
        setPicks(Object.fromEntries(p.conflicts.map((c) => [conflictKey(c), c.default_source])));
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const resolutions = useMemo<MigrationConflictResolution[]>(
    () =>
      (plan?.conflicts ?? []).map((c) => ({
        kind: c.kind,
        name: c.name,
        source_profile: picks[conflictKey(c)] ?? c.default_source,
      })),
    [plan, picks],
  );

  const apply = async () => {
    setApplying(true);
    setError(null);
    try {
      await migrationApply(resolutions, LIBRARY);
      setApplied(true);
    } catch (e) {
      setError(String(e));
    }
    setApplying(false);
  };

  const quit = async () => {
    try {
      await appQuit();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <WizardDialog
      plan={plan}
      picks={picks}
      error={error}
      applying={applying}
      applied={applied}
      onPick={(key, source) => setPicks((prev) => ({ ...prev, [key]: source }))}
      onApply={() => void apply()}
      onQuit={() => void quit()}
      onClose={onClose}
    />
  );
}

export interface WizardDialogProps {
  /** Null while Vosh reads your profiles. */
  plan: MigrationPlan | null;
  picks: Record<string, string>;
  error: string | null;
  applying: boolean;
  applied: boolean;
  onPick: (key: string, source: string) => void;
  onApply: () => void;
  onQuit: () => void;
  onClose: () => void;
}

/** The dialog for a state of the preview. Focus starts on its first
 *  control and moves to the new first control as the plan loads and as
 *  Apply finishes. Esc and a press outside cancel, except while Apply
 *  runs. Enter on the card's text does nothing, so only the Apply
 *  button applies. */
export function WizardDialog({
  plan,
  picks,
  error,
  applying,
  applied,
  onPick,
  onApply,
  onQuit,
  onClose,
}: WizardDialogProps) {
  const titleId = useId();
  const bodyId = useId();
  const cardRef = useRef<HTMLDivElement | null>(null);
  const cancel = () => {
    if (!applying) onClose();
  };
  useEscape(true, cancel);

  useEffect(() => {
    const card = cardRef.current;
    return card ? trapDialogFocus(document, card, () => undefined) : undefined;
  }, []);

  const phase = applied ? 'done' : plan ? 'plan' : 'loading';
  useEffect(() => {
    cardRef.current?.querySelector<HTMLElement>(DIALOG_FOCUSABLE)?.focus({ preventScroll: true });
  }, [phase]);

  return (
    <div
      className="ov-confirm-layer ov-wizard-layer"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) cancel();
      }}
      onMouseUp={(e) => e.stopPropagation()}
    >
      <div
        ref={cardRef}
        className="ov-confirm ov-wizard"
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
      >
        <div className="ov-wizard-head">
          <h2 id={titleId} className="ov-confirm-title">
            {applied ? 'Your catalog is saved' : 'Share one catalog'}
          </h2>
          <p id={bodyId} className="ov-confirm-body">
            {applied ? DONE_BODY : introLine(plan)}
          </p>
        </div>
        {(!applied || error) && (
          <div className="st-content ov-wizard-body">
            {error && (
              <Card>
                <CardNote tone="warn">{error}</CardNote>
              </Card>
            )}
            {applied ? null : plan ? (
              <PlanView plan={plan} picks={picks} onPick={onPick} disabled={applying} />
            ) : (
              !error && <CardNote>Reading your profiles…</CardNote>
            )}
          </div>
        )}
        <div className="ov-wizard-foot">
          {applied ? (
            <>
              <Button onClick={onClose}>Close</Button>
              <Button variant="primary" onClick={onQuit}>
                Quit Vosh
              </Button>
            </>
          ) : (
            <>
              <span className="st-row-desc">
                Vosh keeps a copy of each profile, then asks you to reopen it.
              </span>
              <Button disabled={applying} onClick={onClose}>
                Cancel
              </Button>
              <Button variant="primary" disabled={!plan || applying} onClick={onApply}>
                {applying ? 'Applying…' : 'Apply'}
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

const DONE_BODY =
  'Vosh saved the catalog and a loadout for each character. Nothing you change now saves until you reopen Vosh, and your old profiles wait in profiles/legacy.';

/** The head's line, which names the profiles once the plan names them. */
function introLine(plan: MigrationPlan | null): string {
  const names = plan && plan.source_profiles.length > 0 ? plan.source_profiles : null;
  const whose = names ? `of ${listJoin(names)}` : 'of your profiles';
  return `Vosh merges the aliases, triggers and macros ${whose} into one catalog, with a loadout for each character. Nothing changes until you apply it.`;
}

interface PlanViewProps {
  plan: MigrationPlan;
  picks: Record<string, string>;
  onPick: (key: string, source: string) => void;
  disabled: boolean;
}

/** The three sections of the plan. */
export function PlanView({ plan, picks, onPick, disabled }: PlanViewProps) {
  const { aliases, triggers, macros } = plan.auto_resolved;
  const changes = presetChanges(plan);
  return (
    <>
      {plan.conflicts.length > 0 && (
        <Section
          title="Pick the version to keep"
          actions={<span className="st-meta">{plan.conflicts.length} to pick</span>}
        >
          {plan.conflicts.map((c) => {
            const key = conflictKey(c);
            const chosen = picks[key] ?? c.default_source;
            const options = c.variants.map((v) => ({
              value: v.source_profile,
              label: v.source_profile,
              disabled,
            }));
            return (
              <Row key={key} label={c.name} description={conflictLine(c)}>
                {options.length > MOST_SEGMENTS ? (
                  <Select
                    value={chosen}
                    disabled={disabled}
                    options={options}
                    onChange={(source) => onPick(key, source)}
                  />
                ) : (
                  <Segmented
                    value={chosen}
                    options={options}
                    onChange={(source) => onPick(key, source)}
                  />
                )}
              </Row>
            );
          })}
        </Section>
      )}

      <Section title="Merged as they are">
        <Row label="Aliases">
          <span className="st-meta">{aliases.length}</span>
        </Row>
        <Row label="Triggers" description={namesLine(triggers.map((t) => t.name))}>
          <span className="st-meta">{triggers.length}</span>
        </Row>
        <Row label="Macros">
          <span className="st-meta">{macros.length}</span>
        </Row>
      </Section>

      <Section title="A loadout for each character">
        {plan.loadouts.map((l) => {
          const change = changes.find((c) => c.profile === l.name);
          return (
            <Row key={l.name} label={l.name} description={change && presetLine(change)}>
              <span className="st-meta">
                {l.enabled_groups.length > 0
                  ? `Turns on ${listJoin(l.enabled_groups)}`
                  : 'Turns on no groups'}
              </span>
            </Row>
          );
        })}
      </Section>
    </>
  );
}

/** A few names, then how many more. */
function namesLine(names: string[]): string {
  if (names.length <= MOST_NAMES) return names.join(', ');
  return `${names.slice(0, MOST_NAMES).join(', ')} and ${names.length - MOST_NAMES} more`;
}

/** The presets a character gains and loses once every character shares
 *  one preset list, like `Gains Herb labels.` */
function presetLine(change: { gains: string[]; loses: string[] }): string {
  return [
    change.gains.length > 0 && `Gains ${listJoin(change.gains)}.`,
    change.loses.length > 0 && `Loses ${listJoin(change.loses)}.`,
  ]
    .filter(Boolean)
    .join(' ');
}

const KIND_NAMES: Record<MigrationItemKind, string> = {
  alias: 'Alias',
  trigger: 'Trigger',
  macro: 'Macro',
  preset: 'Preset',
};

const KIND_VERBS: Record<MigrationItemKind, string> = {
  alias: 'sends',
  trigger: 'matches',
  macro: 'sends',
  preset: 'changes',
};

/** A conflict's kind and what differs between its versions, like
 *  `Macro. Maren sends look, Orla sends scan.` or, for versions that
 *  differ only in whether they are on, `Trigger. On for Tolliver, off
 *  for Maren.` */
function conflictLine(c: MigrationConflict): string {
  const bodies = c.variants.map((v) => summarizeVariant(c.kind, c.name, v));
  const on = c.variants.filter((v) => v.switched_on).map((v) => v.source_profile);
  const off = c.variants.filter((v) => !v.switched_on).map((v) => v.source_profile);
  let differs: string;
  if (new Set(bodies).size > 1) {
    differs = c.variants
      .map((v, n) => `${v.source_profile} ${KIND_VERBS[c.kind]} ${bodies[n] || 'nothing'}`)
      .join(', ');
  } else if (on.length > 0 && off.length > 0) {
    differs = `On for ${listJoin(on)}, off for ${listJoin(off)}`;
  } else {
    differs = 'Each profile has its own version';
  }
  return `${KIND_NAMES[c.kind]}. ${differs}${/[.!?…]$/.test(differs) ? '' : '.'}`;
}

function conflictKey(c: { kind: MigrationItemKind; name: string }): string {
  return `${c.kind}::${c.name}`;
}

function summarizeVariant(
  kind: MigrationItemKind,
  name: string,
  v: { item: { kind: MigrationItemKind; item: Record<string, unknown> } },
): string {
  const item = v.item.item;
  if (kind === 'alias') return clip((item.expansion ?? '') as string);
  if (kind === 'trigger') {
    const patterns = (item.patterns ?? []) as TriggerPattern[];
    return clip(patterns.length > 0 ? patternSource(patterns[0]) : '');
  }
  if (kind === 'preset') return versionChanges(name, item as PresetEdit);
  return clip((item.command ?? '') as string);
}

function clip(text: string): string {
  return text.length > 80 ? `${text.slice(0, 80)}…` : text;
}

/** What a version of a preset changed, as the Your changes line of the
 *  preset's card names it, inside a sentence, like `the line color and
 *  buff.sanctuary`. */
function versionChanges(id: string, edit: PresetEdit): string {
  const preset = presetById(id);
  const line = preset && changesLine(preset, edit);
  if (!line) return '';
  if ('count' in line) return line.count;
  return listJoin([...line.colors, ...line.triggers]).replace(/^The /, 'the ');
}
