import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { MigrationPlan } from '../../ipc/wizard';
import { AppliedNotice, PlanView } from './MigrationWizard';

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
  shared_presets: ['healing_basics', 'herb_labels'],
  profile_presets: [['healing_basics'], ['healing_basics', 'herb_labels']],
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

  it('names what each version of a preset changed, as its card does', () => {
    const line = { value: '#8fa7d9', was: 'fg:178' };
    const off = { enabled: { value: false, was: true } };
    const html = draw({
      ...PLAN,
      conflicts: [
        {
          kind: 'preset',
          name: 'disarm_buff_fade',
          default_source: 'default',
          variants: [
            {
              source_profile: 'default',
              switched_on: true,
              item: {
                kind: 'preset',
                item: { colors: { line }, triggers: { 'buff.sanctuary': off } },
              },
            },
            {
              source_profile: 'Healer',
              switched_on: true,
              item: {
                kind: 'preset',
                item: {
                  colors: { line },
                  triggers: { 'buff.sanctuary': off, 'buff.stoneskin': off },
                },
              },
            },
          ],
        },
      ],
    });
    const bodies = [...html.matchAll(/migration-variant-body">([^<]*)</g)].map((m) => m[1]);
    expect(bodies).toEqual(['The line color, buff.sanctuary', '1 color and 2 triggers']);
  });
});

describe('the shared presets in the preview', () => {
  it('says the preset list is shared and who gains or loses which preset', () => {
    const html = draw(PLAN);
    expect(html).toContain(
      'Loadout mode keeps one list of presets that are on, and every character shares it.',
    );
    // Default gains the herb labels, and the Healer keeps what it had.
    expect(html).toContain(
      '<span class="migration-loadout-name">default</span><span class="migration-preset-verb">gains</span><span class="migration-group-tag">Herb labels</span>',
    );
    expect(html).not.toContain(
      '<span class="migration-loadout-name">Healer</span><span class="migration-preset-verb">',
    );
  });

  it('says so when no character gains or loses a preset', () => {
    const html = draw({
      ...PLAN,
      shared_presets: ['healing_basics'],
      profile_presets: [['healing_basics'], ['healing_basics']],
    });
    expect(html).toContain('Every character has the same presets on as now.');
  });
});

describe('the wizard once the move is done', () => {
  const html = renderToStaticMarkup(<AppliedNotice />);

  it('says the preset list is shared', () => {
    expect(html).toContain('Every character now shares one list of presets that are on.');
    expect(html).not.toContain('kept every other setting');
  });

  it('says Vosh saves nothing you change before you quit', () => {
    expect(html).toContain(
      'Vosh does not save the changes you make before you quit, so quit Vosh below and open it again to use the catalog.',
    );
    expect(html).not.toMatch(/[;‒-―]/);
  });
});
