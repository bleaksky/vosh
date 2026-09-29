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
  });
});
