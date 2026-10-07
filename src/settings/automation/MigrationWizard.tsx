import { useEffect, useMemo, useState } from 'react';
import { type TriggerPattern } from '../../ipc/automation';
import type { PresetEdit } from '../../ipc/presetEdits';
import { appQuit } from '../../ipc/windows';
import {
  migrationAnalyze,
  migrationApply,
  type MigrationConflictResolution,
  type MigrationItemKind,
  type MigrationPlan,
} from '../../ipc/wizard';
import { patternSource } from '../../automation/automationTriggers';
import { changesLine } from '../../automation/presetEdits';
import { PRESETS, presetById } from '../../automation/presets';
import { presetChanges } from '../../automation/wizardPresets';

/** The id of every preset in the library this build installs from. */
const LIBRARY = PRESETS.map((p) => p.id);

interface Props {
  onClose: () => void;
}

// Wizard for the Path B migration. Shows the analyzer's plan in three
// sections (auto-resolved, conflicts, derived loadouts), lets the user
// pick a winner per conflict via a radio per source, then runs the
// apply step which copies the per-profile files into profiles/legacy/,
// writes catalog.toml + loadouts.toml, and takes the aliases, triggers,
// and macros out of each profile file, which keeps every other setting.
// Every character then shares one preset list, and the preview says who
// gains or loses a preset by it.
// The runtime stays in legacy mode until the user relaunches Vosh: the
// wizard switches to a
// "Migration complete" state with a [quit Vosh] button. Path B mode
// activates on the next launch when the startup hook picks up the
// freshly-written catalog.toml.
export function MigrationWizard({ onClose }: Props) {
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(true);
  // Map of conflict-key -> chosen source profile. Missing entries
  // fall back to the analyzer's default, the one version that was on
  // when exactly one was.
  const [picks, setPicks] = useState<Record<string, string>>({});
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const p = await migrationAnalyze(LIBRARY);
        if (!cancelled) {
          setPlan(p);
          setPending(false);
          // Seed picks with each conflict's default so the submission
          // payload is explicit even when the user does not interact.
          const seed: Record<string, string> = {};
          for (const c of p.conflicts) {
            seed[conflictKey(c.kind, c.name)] = c.default_source;
          }
          setPicks(seed);
        }
      } catch (e) {
        if (!cancelled) {
          setError(String(e));
          setPending(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const resolutions = useMemo<MigrationConflictResolution[]>(() => {
    if (!plan) return [];
    return plan.conflicts.map((c) => ({
      kind: c.kind,
      name: c.name,
      source_profile: picks[conflictKey(c.kind, c.name)] ?? c.default_source,
    }));
  }, [plan, picks]);

  const handleApply = async () => {
    if (!plan) return;
    setApplying(true);
    setError(null);
    try {
      await migrationApply(resolutions, LIBRARY);
      setApplied(true);
      setApplying(false);
    } catch (e) {
      setError(String(e));
      setApplying(false);
    }
  };

  const handleQuit = async () => {
    try {
      await appQuit();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="migration-wizard-backdrop" onClick={onClose}>
      <div
        className="migration-wizard"
        role="dialog"
        aria-label="Path B migration"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="migration-wizard-header">
          <span className="migration-wizard-title">migrate to global catalog</span>
          <button type="button" className="migration-wizard-close" onClick={onClose}>
            close
          </button>
        </header>

        <div className="migration-wizard-body">
          {pending && <div className="migration-wizard-status">analyzing profiles...</div>}
          {error && <div className="migration-wizard-error">[error] {error}</div>}
          {applied && <AppliedNotice />}
          {!applied && plan && (
            <PlanView
              plan={plan}
              picks={picks}
              onPick={(key, source) => setPicks((prev) => ({ ...prev, [key]: source }))}
              disabled={applying}
            />
          )}
        </div>

        <footer className="migration-wizard-footer">
          {applied ? (
            <>
              <span className="migration-wizard-hint">
                Path B activates the next time you launch Vosh.
              </span>
              <button
                type="button"
                className="settings-btn migration-apply-btn"
                onClick={() => void handleQuit()}
              >
                quit Vosh
              </button>
            </>
          ) : (
            <>
              <span className="migration-wizard-hint">
                Applying moves your aliases, triggers, and macros into one shared catalog, and every
                character then shares one list of presets that are on. Every other setting stays
                with its profile, and Vosh copies each profile file to profiles/legacy first.
              </span>
              <button
                type="button"
                className="settings-btn migration-apply-btn"
                disabled={!plan || applying}
                onClick={() => void handleApply()}
              >
                {applying ? 'applying...' : 'apply migration'}
              </button>
            </>
          )}
        </footer>
      </div>
    </div>
  );
}

/** What the wizard says once it wrote its files. Nothing the session
 *  changes saves until Vosh opens again, and the main window says so too. */
export function AppliedNotice() {
  return (
    <div className="migration-wizard-status migration-wizard-applied">
      <div className="migration-wizard-applied-title">migration complete.</div>
      <div className="migration-wizard-applied-body">
        Vosh saved the shared catalog and a loadout for each profile. Every character now shares one
        list of presets that are on. Each profile kept its other settings, and a full copy of each
        old profile file waits in profiles/legacy. Vosh does not save the changes you make before
        you quit, so quit Vosh below and open it again to use the catalog.
      </div>
    </div>
  );
}

interface PlanViewProps {
  plan: MigrationPlan;
  picks: Record<string, string>;
  onPick: (key: string, source: string) => void;
  disabled: boolean;
}

export function PlanView({ plan, picks, onPick, disabled }: PlanViewProps) {
  const autoResolvedTotal =
    plan.auto_resolved.aliases.length +
    plan.auto_resolved.triggers.length +
    plan.auto_resolved.macros.length;
  return (
    <>
      <Section title="source profiles">
        {plan.source_profiles.length === 0 ? (
          <Empty>no profiles found</Empty>
        ) : (
          <ul className="migration-list">
            {plan.source_profiles.map((p) => (
              <li key={p} className="migration-list-item">
                {p}
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section title={`auto-resolved (${autoResolvedTotal})`}>
        <div className="migration-counts">
          <CountChip label="aliases" count={plan.auto_resolved.aliases.length} />
          <CountChip label="triggers" count={plan.auto_resolved.triggers.length} />
          <CountChip label="macros" count={plan.auto_resolved.macros.length} />
        </div>
        <div className="migration-hint">
          Items only one profile has, and items each profile that has them holds with the same
          content. Copies that differ only in their folder or in whether they are on become one
          item, and each profile keeps it on or off as it had it. A trigger that differs between
          profiles keeps each version, and the copies of a preset trigger become the library
          version.
        </div>
      </Section>

      <Section title={`conflicts (${plan.conflicts.length})`}>
        {plan.conflicts.length === 0 ? (
          <Empty>
            No conflicts. Every alias and macro is the same in each profile that has it, apart from
            its folder and whether it is on.
          </Empty>
        ) : (
          <ul className="migration-conflict-list">
            {plan.conflicts.map((c) => {
              const key = conflictKey(c.kind, c.name);
              const chosen = picks[key] ?? c.default_source;
              return (
                <li key={key} className="migration-conflict">
                  <div className="migration-conflict-head">
                    <span className={`migration-kind-tag migration-kind-${c.kind}`}>
                      {kindLabel(c.kind)}
                    </span>
                    <span className="migration-conflict-name">{c.name}</span>
                  </div>
                  <ul className="migration-variant-list">
                    {c.variants.map((v) => (
                      <li key={v.source_profile} className="migration-variant">
                        <label className="migration-variant-radio">
                          <input
                            type="radio"
                            name={key}
                            value={v.source_profile}
                            checked={chosen === v.source_profile}
                            onChange={() => onPick(key, v.source_profile)}
                            disabled={disabled}
                          />
                          <span className="migration-variant-source">{v.source_profile}</span>
                          <span
                            className={`migration-variant-state${v.switched_on ? ' is-on' : ''}`}
                          >
                            {v.switched_on ? 'on' : 'off'}
                          </span>
                        </label>
                        <span className="migration-variant-body">
                          {summarizeVariant(c.kind, c.name, v)}
                        </span>
                      </li>
                    ))}
                  </ul>
                </li>
              );
            })}
          </ul>
        )}
        {plan.conflicts.length > 0 && (
          <div className="migration-hint">
            Pick the version to keep. When only one version is on, the wizard picks it for you. The
            copies in profiles/legacy keep every version.
          </div>
        )}
      </Section>

      <Section title={`derived loadouts (${plan.loadouts.length})`}>
        {plan.loadouts.length === 0 ? (
          <Empty>no loadouts would be created.</Empty>
        ) : (
          <ul className="migration-loadout-list">
            {plan.loadouts.map((l) => (
              <li key={l.name} className="migration-loadout">
                <div className="migration-loadout-head">
                  <span className="migration-loadout-name">{l.name}</span>
                  {l.description && <span className="migration-loadout-desc">{l.description}</span>}
                </div>
                <div className="migration-loadout-groups">
                  {l.enabled_groups.length === 0 ? (
                    <span className="migration-hint-inline">(no groups)</span>
                  ) : (
                    l.enabled_groups.map((g) => (
                      <span key={g} className="migration-group-tag">
                        {g}
                      </span>
                    ))
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
        <div className="migration-hint">
          Each loadout turns on the groups its profile had on. Every loadout starts off, so each
          profile keeps on the items it has on now, at launch and when you switch.
        </div>
      </Section>

      <SharedPresets plan={plan} />
    </>
  );
}

/** Who gains and who loses which preset once every character shares one
 *  preset list. */
function SharedPresets({ plan }: { plan: MigrationPlan }) {
  const changes = presetChanges(plan);
  const tags = (names: string[]) =>
    names.map((name) => (
      <span key={name} className="migration-group-tag">
        {name}
      </span>
    ));
  return (
    <Section title="shared presets">
      {changes.length === 0 ? (
        <Empty>Every character has the same presets on as now.</Empty>
      ) : (
        <ul className="migration-loadout-list">
          {changes.map((c) => (
            <li key={c.profile} className="migration-loadout migration-preset-change">
              <span className="migration-loadout-name">{c.profile}</span>
              {c.gains.length > 0 && (
                <>
                  <span className="migration-preset-verb">gains</span>
                  {tags(c.gains)}
                </>
              )}
              {c.loses.length > 0 && (
                <>
                  <span className="migration-preset-verb">loses</span>
                  {tags(c.loses)}
                </>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="migration-hint">
        Loadout mode keeps one list of presets that are on, and every character shares it. The list
        holds every preset that any profile file has on now. A profile that never saved a file has
        every preset on.
      </div>
    </Section>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="migration-section">
      <h3 className="migration-section-title">{title}</h3>
      {children}
    </section>
  );
}

function CountChip({ label, count }: { label: string; count: number }) {
  return (
    <span className="migration-count-chip">
      <span className="migration-count-chip-n">{count}</span>
      <span className="migration-count-chip-l">{label}</span>
    </span>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return <div className="migration-empty">{children}</div>;
}

function kindLabel(kind: MigrationItemKind): string {
  return kind;
}

function conflictKey(kind: MigrationItemKind, name: string): string {
  return `${kind}::${name}`;
}

function summarizeVariant(
  kind: MigrationItemKind,
  name: string,
  v: { item: { kind: MigrationItemKind; item: Record<string, unknown> } },
): string {
  const item = v.item.item;
  if (kind === 'alias') {
    const expansion = (item.expansion ?? '') as string;
    return expansion.length > 80 ? `${expansion.slice(0, 80)}…` : expansion;
  }
  if (kind === 'trigger') {
    const patterns = (item.patterns ?? []) as TriggerPattern[];
    const first = patterns.length > 0 ? patternSource(patterns[0]) : '';
    return first.length > 80 ? `${first.slice(0, 80)}…` : first;
  }
  if (kind === 'preset') return versionChanges(name, item as PresetEdit);
  const command = (item.command ?? '') as string;
  return command.length > 80 ? `${command.slice(0, 80)}…` : command;
}

/** What a version of a preset changed, as the Your changes line of the
 *  preset's card names it, like `The line color, buff.sanctuary`. */
function versionChanges(id: string, edit: PresetEdit): string {
  const preset = presetById(id);
  const line = preset && changesLine(preset, edit);
  if (!line) return '';
  return 'count' in line ? line.count : [...line.colors, ...line.triggers].join(', ');
}
