import { useEffect, useMemo, useState } from 'react';
import {
  appQuit,
  migrationAnalyze,
  migrationApply,
  type MigrationConflictResolution,
  type MigrationItemKind,
  type MigrationPlan,
} from '../lib/session';

interface Props {
  onClose: () => void;
}

// Wizard for the Path B migration. Shows the analyzer's plan in three
// sections (auto-resolved, conflicts, derived loadouts), lets the user
// pick a winner per conflict via a radio per source, then runs the
// apply step which writes catalog.toml + loadouts.toml and moves the
// per-profile files into profiles/legacy/. The runtime stays in legacy
// mode until the user relaunches Vosh: the wizard switches to a
// "Migration complete" state with a [quit Vosh] button. Path B mode
// activates on the next launch when the startup hook picks up the
// freshly-written catalog.toml.
export function MigrationWizard({ onClose }: Props) {
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(true);
  // Map of conflict-key -> chosen source profile. Missing entries
  // fall back to the first variant (the analyzer's default).
  const [picks, setPicks] = useState<Record<string, string>>({});
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const p = await migrationAnalyze();
        if (!cancelled) {
          setPlan(p);
          setPending(false);
          // Seed picks with each conflict's first variant so the
          // submission payload is explicit even when the user does
          // not interact.
          const seed: Record<string, string> = {};
          for (const c of p.conflicts) {
            const first = c.variants[0]?.source_profile;
            if (first) seed[conflictKey(c.kind, c.name)] = first;
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
      source_profile: picks[conflictKey(c.kind, c.name)] ?? c.variants[0].source_profile,
    }));
  }, [plan, picks]);

  const handleApply = async () => {
    if (!plan) return;
    setApplying(true);
    setError(null);
    try {
      await migrationApply(resolutions);
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
            [close]
          </button>
        </header>

        <div className="migration-wizard-body">
          {pending && <div className="migration-wizard-status">analyzing profiles...</div>}
          {error && <div className="migration-wizard-error">[error] {error}</div>}
          {applied && (
            <div className="migration-wizard-status migration-wizard-applied">
              <div className="migration-wizard-applied-title">migration complete.</div>
              <div className="migration-wizard-applied-body">
                catalog.toml and loadouts.toml are on disk; your per-profile files are preserved
                under profiles/legacy/ in case you want to roll back. Vosh stays in legacy mode
                until you quit and relaunch. Click [quit Vosh] below, then reopen Vosh to enter Path
                B.
              </div>
            </div>
          )}
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
                [quit Vosh]
              </button>
            </>
          ) : (
            <>
              <span className="migration-wizard-hint">
                applying writes catalog.toml + loadouts.toml and moves per-profile files into
                profiles/legacy/. Vosh stays in legacy mode until you relaunch.
              </span>
              <button
                type="button"
                className="settings-btn migration-apply-btn"
                disabled={!plan || applying}
                onClick={() => void handleApply()}
              >
                {applying ? '[applying...]' : '[apply migration]'}
              </button>
            </>
          )}
        </footer>
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

function PlanView({ plan, picks, onPick, disabled }: PlanViewProps) {
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
          items present in exactly one source profile, or identical across every source after group
          retagging. these collapse into a single entry in the catalog with no user input needed.
        </div>
      </Section>

      <Section title={`conflicts (${plan.conflicts.length})`}>
        {plan.conflicts.length === 0 ? (
          <Empty>no conflicts — every named item is either unique or byte-identical.</Empty>
        ) : (
          <ul className="migration-conflict-list">
            {plan.conflicts.map((c) => {
              const key = conflictKey(c.kind, c.name);
              const chosen = picks[key] ?? c.variants[0].source_profile;
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
                        </label>
                        <span className="migration-variant-body">
                          {summarizeVariant(c.kind, v)}
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
            pick the variant you want preserved. unchecked profiles keep their version inside
            profiles/legacy/ so nothing is permanently lost.
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
          each loadout enables the groups its source profile contributed. the previously-active
          profile becomes the sole initial active loadout so your day-one session matches today.
        </div>
      </Section>
    </>
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
  v: { item: { kind: MigrationItemKind; item: Record<string, unknown> } },
): string {
  const item = v.item.item;
  if (kind === 'alias') {
    const expansion = (item.expansion ?? '') as string;
    return expansion.length > 80 ? `${expansion.slice(0, 80)}…` : expansion;
  }
  if (kind === 'trigger') {
    const patterns = (item.patterns ?? []) as Array<{ pattern: string }>;
    const first = patterns.length > 0 ? patterns[0].pattern : '';
    return first.length > 80 ? `${first.slice(0, 80)}…` : first;
  }
  const command = (item.command ?? '') as string;
  return command.length > 80 ? `${command.slice(0, 80)}…` : command;
}
