import { useEffect, useState } from 'react';
import { migrationAnalyze, type MigrationPlan, type MigrationItemKind } from '../lib/session';

interface Props {
  onClose: () => void;
}

// Read-only preview of the Path B migration. Shows source profiles,
// auto-resolved item counts per kind, the per-source-profile loadouts
// that would be generated, and any conflicts the user would need to
// pick winners on. No apply button yet — that wires in a follow-up
// commit once this UX is validated.
export function MigrationWizard({ onClose }: Props) {
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(true);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const p = await migrationAnalyze();
        if (!cancelled) {
          setPlan(p);
          setPending(false);
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

  return (
    <div className="migration-wizard-backdrop" onClick={onClose}>
      <div
        className="migration-wizard"
        role="dialog"
        aria-label="Path B migration preview"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="migration-wizard-header">
          <span className="migration-wizard-title">migrate to global catalog (preview)</span>
          <button type="button" className="migration-wizard-close" onClick={onClose}>
            [close]
          </button>
        </header>

        <div className="migration-wizard-body">
          {pending && <div className="migration-wizard-status">analyzing profiles...</div>}
          {error && <div className="migration-wizard-error">[error] {error}</div>}
          {plan && <PlanView plan={plan} />}
        </div>

        <footer className="migration-wizard-footer">
          <span className="migration-wizard-hint">
            preview only — nothing is written to disk by this view.
          </span>
        </footer>
      </div>
    </div>
  );
}

function PlanView({ plan }: { plan: MigrationPlan }) {
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
            {plan.conflicts.map((c) => (
              <li key={`${c.kind}-${c.name}`} className="migration-conflict">
                <div className="migration-conflict-head">
                  <span className={`migration-kind-tag migration-kind-${c.kind}`}>
                    {kindLabel(c.kind)}
                  </span>
                  <span className="migration-conflict-name">{c.name}</span>
                </div>
                <ul className="migration-variant-list">
                  {c.variants.map((v) => (
                    <li key={v.source_profile} className="migration-variant">
                      <span className="migration-variant-source">{v.source_profile}</span>
                      <span className="migration-variant-body">{summarizeVariant(c.kind, v)}</span>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        )}
        {plan.conflicts.length > 0 && (
          <div className="migration-hint">
            the apply step (not in this build) will let you pick which variant wins, or keep all
            variants under renamed slots.
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
          each loadout enables the groups its source profile contributed. activate one and its
          aliases / triggers / macros become live; deactivate it and they pass through.
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
  // macro
  const command = (item.command ?? '') as string;
  return command.length > 80 ? `${command.slice(0, 80)}…` : command;
}
