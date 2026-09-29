import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { MigrationPlan } from '../lib/session';
import { PlanView } from './MigrationWizard';

// The wizard reaches the Tauri bridge only when it analyzes or applies.
// PlanView, under test, draws from the plan it is given.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// Default and the Healer both have kk, the Healer with it turned off, and
// each has its own version of the greet trigger.
const PLAN: MigrationPlan = {
  source_profiles: ['default', 'Healer'],
  auto_resolved: {
    aliases: [{ name: 'kk', group: '(default)' }],
    triggers: [
      { name: 'greet', group: '(default)' },
      { name: 'greet (Healer)', group: '(Healer)' },
    ],
    macros: [],
  },
  conflicts: [],
  loadouts: [
    { name: 'default', enabled_groups: ['(default)'] },
    { name: 'Healer', enabled_groups: ['(Healer)'] },
  ],
};

function draw(plan: MigrationPlan): string {
  return renderToStaticMarkup(
    <PlanView plan={plan} picks={{}} onPick={() => undefined} disabled={false} />,
  );
}

describe('the shared catalog preview', () => {
  it('says what folds into one item without a question', () => {
    const html = draw(PLAN);
    // Copies that differ in whether they are on, and triggers with
    // versions of their own, fold without a question, so the preview
    // no longer calls every item unique or byte identical.
    expect(html).not.toMatch(/byte-identical|group\s+retagging/);
    expect(html).toContain('whether they are on');
    expect(html).toContain('A trigger that differs between profiles keeps each version');
    expect(html).toContain('No conflicts. Every alias and macro is the same in each profile');
  });

  it('writes no dashes', () => {
    expect(draw(PLAN)).not.toMatch(/[‒-―]/);
    expect(draw(CONFLICT_PLAN)).not.toMatch(/[‒-―]/);
  });
});

// Default kept its kk off, and only the Healer's version was on.
const CONFLICT_PLAN: MigrationPlan = {
  ...PLAN,
  auto_resolved: { aliases: [], triggers: [], macros: [] },
  conflicts: [
    {
      kind: 'alias',
      name: 'kk',
      default_source: 'Healer',
      variants: [
        {
          source_profile: 'default',
          switched_on: false,
          item: { kind: 'alias', item: { name: 'kk', expansion: 'kick %1' } },
        },
        {
          source_profile: 'Healer',
          switched_on: true,
          item: { kind: 'alias', item: { name: 'kk', expansion: 'kick 1.' } },
        },
      ],
    },
  ],
};

describe('a conflict in the shared catalog preview', () => {
  it('picks the one version that was on until you pick another', () => {
    const checked = (draw(CONFLICT_PLAN).match(/<input[^>]*>/g) ?? [])
      .filter((input) => input.includes('checked=""'))
      .map((input) => /value="([^"]*)"/.exec(input)?.[1]);
    expect(checked).toEqual(['Healer']);
  });

  it('shows which version each profile had on', () => {
    const html = draw(CONFLICT_PLAN);
    expect(html).toContain('Healer</span><span class="migration-variant-state is-on">on</span>');
    expect(html).toContain('default</span><span class="migration-variant-state">off</span>');
    expect(html).toContain('When only one version is on, the wizard picks it for you.');
  });
});
