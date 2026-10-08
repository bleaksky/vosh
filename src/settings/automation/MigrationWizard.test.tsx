import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { MigrationConflict, MigrationPlan } from '../../ipc/wizard';
import { WizardDialog, type WizardDialogProps } from './MigrationWizard';

// The preview as markup. Effects do not run here, so neither the plan
// read nor the focus trap starts, and each test hands the dialog the
// state it draws.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const trigger = (pattern: string) => ({
  name: 'Recast armor',
  patterns: [{ kind: 'substring', pattern }],
  command: 'cast armor',
});

// Tolliver had Recast armor on and Maren off, and Maren's F1 looks
// where Orla's scans. Orla keeps the Herb labels Tolliver turns on.
const PLAN: MigrationPlan = {
  source_profiles: ['Tolliver', 'Maren', 'Orla'],
  auto_resolved: {
    aliases: [{ name: 'eb', group: null }],
    triggers: [
      { name: 'Eat when hungry', group: 'idle' },
      { name: 'Drink when thirsty', group: 'idle' },
    ],
    macros: [{ key: 'F2', group: null }],
  },
  conflicts: [
    {
      kind: 'trigger',
      name: 'Recast armor',
      default_source: 'Tolliver',
      variants: [
        {
          source_profile: 'Tolliver',
          switched_on: true,
          item: { kind: 'trigger', item: trigger('You feel less protected.') },
        },
        {
          source_profile: 'Maren',
          switched_on: false,
          item: { kind: 'trigger', item: trigger('You feel less protected.') },
        },
      ],
    },
    {
      kind: 'macro',
      name: 'F1',
      default_source: 'Orla',
      variants: [
        {
          source_profile: 'Maren',
          switched_on: true,
          item: { kind: 'macro', item: { key: 'F1', command: 'look' } },
        },
        {
          source_profile: 'Orla',
          switched_on: true,
          item: { kind: 'macro', item: { key: 'F1', command: 'scan' } },
        },
      ],
    },
  ],
  loadouts: [
    { name: 'Tolliver', enabled_groups: ['combat'] },
    { name: 'Maren', enabled_groups: ['combat', 'idle'] },
    { name: 'Orla', enabled_groups: [] },
  ],
  shared_presets: ['healing_basics', 'herb_labels'],
  profile_presets: [
    ['healing_basics', 'herb_labels'],
    ['healing_basics', 'herb_labels', 'disarm_buff_fade'],
    ['healing_basics'],
  ],
};

const none = () => undefined;

function draw(state: Partial<WizardDialogProps> = {}): string {
  return renderToStaticMarkup(
    <WizardDialog
      plan={PLAN}
      picks={{}}
      error={null}
      applying={false}
      applied={false}
      onPick={none}
      onApply={none}
      onQuit={none}
      onClose={none}
      {...state}
    />,
  );
}

/** Each row's label, description and control text, in order. */
function rows(html: string) {
  return [
    ...html.matchAll(
      /class="st-row-label">([^<]*)<\/label>(?:<span[^>]*class="st-row-desc">([^<]*)<\/span>)?<\/div>(?:<div class="st-row-control">(.*?)<\/div>)?<\/div>/g,
    ),
  ].map(([, label, desc, control]) => ({
    label,
    desc: desc ?? '',
    control: (control ?? '')
      .replace(/<[^>]*>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim(),
  }));
}

/** The pressed segment of the row labelled `label`. */
function pressed(html: string, label: string): string[] {
  const at = html.indexOf(`>${label}</label>`);
  const row = html.slice(at, html.indexOf('class="st-row-label"', at + 1) >>> 0);
  return [...row.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((m) => m[1]);
}

/** The text a player reads, tags gone. */
function text(html: string): string {
  return html.replace(/<[^>]*>/g, ' ');
}

describe('the shared catalog preview', () => {
  it('is a modal dialog named by its head', () => {
    const html = draw();
    expect(html).toMatch(/role="dialog" aria-modal="true" aria-labelledby="[^"]+"/);
    expect(html).toContain('<h2 id="');
    expect(html).toContain('class="ov-confirm-title">Share one catalog</h2>');
    expect(html).toContain(
      'Vosh merges the aliases, triggers and macros of Tolliver, Maren, and Orla into one catalog, with a loadout for each character. Nothing changes until you apply it.',
    );
  });

  it('asks for the version to keep first, with the default pressed', () => {
    const html = draw();
    const sections = [...html.matchAll(/class="st-section-title">([^<]*)</g)].map((m) => m[1]);
    expect(sections).toEqual([
      'Pick the version to keep',
      'Merged as they are',
      'A loadout for each character',
    ]);
    expect(html).toContain('<span class="st-meta">2 to pick</span>');
    expect(pressed(html, 'Recast armor')).toEqual(['Tolliver']);
    expect(pressed(html, 'F1')).toEqual(['Orla']);
    expect(pressed(draw({ picks: { 'macro::F1': 'Maren' } }), 'F1')).toEqual(['Maren']);
    expect(html).not.toContain('type="radio"');
  });

  it('says what kind each item is and what differs', () => {
    const [armor, f1] = rows(draw());
    expect(armor).toEqual({
      label: 'Recast armor',
      desc: 'Trigger. On for Tolliver, off for Maren.',
      control: 'Tolliver Maren',
    });
    expect(f1.desc).toBe('Macro. Maren sends look, Orla sends scan.');
  });

  it('names what each version of a preset changed, as its card does', () => {
    const line = { value: '#8fa7d9', was: 'fg:178' };
    const off = { enabled: { value: false, was: true } };
    const preset: MigrationConflict = {
      kind: 'preset',
      name: 'disarm_buff_fade',
      default_source: 'Tolliver',
      variants: [
        {
          source_profile: 'Tolliver',
          switched_on: true,
          item: { kind: 'preset', item: { colors: { line }, triggers: { 'buff.sanctuary': off } } },
        },
        {
          source_profile: 'Maren',
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
    };
    const [row] = rows(draw({ plan: { ...PLAN, conflicts: [preset] } }));
    expect(row.desc).toBe(
      'Preset. Tolliver changes the line color and buff.sanctuary, Maren changes 1 color and 2 triggers.',
    );
  });

  it('picks from a select past four profiles', () => {
    const names = ['Tolliver', 'Maren', 'Orla', 'Tolliver 2', 'Maren 2'];
    const wide: MigrationConflict = {
      kind: 'macro',
      name: 'F1',
      default_source: 'Orla',
      variants: names.map((name) => ({
        source_profile: name,
        switched_on: true,
        item: { kind: 'macro', item: { key: 'F1', command: `say ${name}` } },
      })),
    };
    const html = draw({ plan: { ...PLAN, conflicts: [wide] } });
    expect(html).not.toContain('st-seg');
    expect(html).toContain('<span class="st-meta">1 to pick</span>');
    const options = [...html.matchAll(/<option value="([^"]*)"( selected="")?/g)];
    expect(options.map((m) => m[1])).toEqual(names);
    expect(options.filter((m) => m[2]).map((m) => m[1])).toEqual(['Orla']);
  });

  it('leaves the pick out when nothing is in conflict', () => {
    const html = draw({ plan: { ...PLAN, conflicts: [] } });
    expect(html).not.toContain('Pick the version to keep');
    expect(html).toContain('Merged as they are');
  });

  it('counts what merges as it is', () => {
    const merged = rows(draw()).slice(2, 5);
    expect(merged).toEqual([
      { label: 'Aliases', desc: '', control: '1' },
      { label: 'Triggers', desc: 'Eat when hungry, Drink when thirsty', control: '2' },
      { label: 'Macros', desc: '', control: '1' },
    ]);
  });

  it('gives each character a loadout and says who gains or loses a preset', () => {
    expect(rows(draw()).slice(5)).toEqual([
      { label: 'Tolliver', desc: '', control: 'Turns on combat' },
      {
        label: 'Maren',
        desc: 'Loses Disarms and fading buffs.',
        control: 'Turns on combat and idle',
      },
      { label: 'Orla', desc: 'Gains Herb labels.', control: 'Turns on no groups' },
    ]);
  });

  it('holds Apply off until the plan loads and while it applies', () => {
    const loading = draw({ plan: null });
    expect(text(loading)).toContain('Reading your profiles…');
    expect(loading).toContain('of your profiles into one catalog');
    expect(loading).toMatch(/<button type="button" disabled="" class="btn is-primary">Apply</);
    const applying = draw({ applying: true });
    expect(applying).toMatch(/disabled="" class="btn is-primary">Applying…</);
    expect(applying).toMatch(/disabled="" class="btn">Cancel</);
    expect(draw()).toMatch(/<button type="button" class="btn is-primary">Apply</);
    expect(text(draw())).toContain(
      'Vosh keeps a copy of each profile, then asks you to reopen it.',
    );
  });

  it('shows an error as a warn note', () => {
    const html = draw({ plan: null, error: 'Vosh could not read the profile Orla.' });
    expect(html).toContain('st-card-note is-warn');
    expect(text(html)).toContain('Vosh could not read the profile Orla.');
    expect(html).not.toContain('Reading your profiles');
  });

  it('offers Quit Vosh once the catalog is saved', () => {
    const html = draw({ applied: true });
    expect(html).toContain('class="ov-confirm-title">Your catalog is saved</h2>');
    expect(text(html)).toContain(
      'Nothing you change now saves until you reopen Vosh, and your old profiles wait in profiles/legacy.',
    );
    const buttons = [...html.matchAll(/class="(btn[^"]*)">([^<]*)</g)].map(
      (m) => `${m[1]} ${m[2]}`,
    );
    expect(buttons).toEqual(['btn Close', 'btn is-primary Quit Vosh']);
    expect(html).not.toContain('st-section');
  });

  it('writes no dashes, semicolons or old wording in any state', () => {
    for (const html of [
      draw(),
      draw({ plan: null }),
      draw({ applying: true }),
      draw({ applied: true }),
      draw({ plan: null, error: 'Vosh could not read the profile Orla.' }),
    ]) {
      const words = text(html);
      expect(words).not.toMatch(/[‐-―;]|\s-\s/);
      expect(words).not.toMatch(/Path B|global catalog/i);
      expect(html).not.toMatch(/migration-|settings-/);
    }
  });
});
