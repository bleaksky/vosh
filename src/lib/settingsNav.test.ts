import { describe, expect, it } from 'vitest';
import {
  formatSettingsTarget,
  leavesSettingsPage,
  resolveSettingsTarget,
  settingsGroupLabel,
  settingsScrollIds,
  settingsSubpage,
  SETTINGS_GROUPS,
  type SettingsTarget,
} from './settingsNav';

describe('resolveSettingsTarget', () => {
  it('maps every tab id the old window used', () => {
    const cases: [string, SettingsTarget][] = [
      ['general', { group: 'general' }],
      ['themes', { group: 'appearance', section: 'theme' }],
      ['typography', { group: 'appearance', section: 'text' }],
      ['vitals', { group: 'vitals' }],
      ['tick', { group: 'automation', section: 'timers', anchor: 'tick' }],
      ['panels', { group: 'characters', anchor: 'layout' }],
      ['profiles', { group: 'characters' }],
      ['loadouts', { group: 'automation', section: 'loadouts' }],
      ['triggers', { group: 'automation', section: 'triggers' }],
      ['aliases', { group: 'automation', section: 'aliases' }],
      ['macros', { group: 'automation', section: 'macros' }],
      ['timers', { group: 'automation', section: 'timers' }],
      ['import', { group: 'automation', anchor: 'import' }],
      ['logs', { group: 'logs', section: 'search' }],
    ];
    for (const [raw, target] of cases) expect(resolveSettingsTarget(raw)).toEqual(target);
  });

  it('reads a bare group in every one of the eleven, but logs', () => {
    expect(SETTINGS_GROUPS).toHaveLength(11);
    for (const { id } of SETTINGS_GROUPS) {
      // The bare link logs opens the search, as it did in General.
      if (id === 'logs') continue;
      expect(resolveSettingsTarget(id)).toEqual({ group: id });
    }
  });

  it('reads a group with a section and an anchor', () => {
    expect(resolveSettingsTarget('automation:macros')).toEqual({
      group: 'automation',
      section: 'macros',
    });
    expect(resolveSettingsTarget('appearance:text#line-height')).toEqual({
      group: 'appearance',
      section: 'text',
      anchor: 'line-height',
    });
    expect(resolveSettingsTarget('automation#import')).toEqual({
      group: 'automation',
      anchor: 'import',
    });
  });

  it('keeps the case of a profile name and lowercases ids', () => {
    expect(resolveSettingsTarget('characters:Ilsabet#tracked')).toEqual({
      group: 'characters',
      section: 'Ilsabet',
      anchor: 'tracked',
    });
    expect(resolveSettingsTarget('characters:Test Prompt')).toEqual({
      group: 'characters',
      section: 'Test Prompt',
    });
    expect(resolveSettingsTarget('Automation:Timers#Tick')).toEqual({
      group: 'automation',
      section: 'timers',
      anchor: 'tick',
    });
    expect(resolveSettingsTarget('  THEMES ')).toEqual({ group: 'appearance', section: 'theme' });
  });

  it('sends the prompt rows that left Input Advanced to the Prompt tab, through two moves', () => {
    expect(resolveSettingsTarget('input:advanced#prompt')).toEqual({ group: 'prompt' });
    expect(resolveSettingsTarget('Input:Advanced#Prompt-Show')).toEqual({
      group: 'prompt',
      anchor: 'prompt-show',
    });
    // Paste pacing stays under Advanced.
    expect(resolveSettingsTarget('input:advanced#paste-delay')).toEqual({
      group: 'input',
      section: 'advanced',
      anchor: 'paste-delay',
    });
    expect(resolveSettingsTarget('input:prompt#prompt-game')).toEqual({
      group: 'prompt',
      anchor: 'prompt-game',
    });
  });

  it('sends the sections and rows that left their tab to the new tabs', () => {
    const cases: [string, SettingsTarget][] = [
      [
        'general:session-logs#keep-logs',
        { group: 'logs', section: 'session-logs', anchor: 'keep-logs' },
      ],
      ['general:scrollback', { group: 'logs', section: 'scrollback' }],
      ['general:logs', { group: 'logs', section: 'search' }],
      ['general:scene', { group: 'logs', section: 'scene' }],
      [
        'appearance:text#color-vision',
        { group: 'accessibility', section: 'color', anchor: 'color-vision' },
      ],
      [
        'appearance:advanced#blink-text',
        { group: 'accessibility', section: 'motion', anchor: 'blink-text' },
      ],
      ['layout:vitals#style', { group: 'vitals', anchor: 'style' }],
      ['layout:vitals#values', { group: 'vitals', section: 'customize-vitals', anchor: 'values' }],
      ['layout:customize-vitals', { group: 'vitals', section: 'customize-vitals' }],
      [
        'input:command-line#writing-offer',
        { group: 'input', section: 'writing', anchor: 'writing-offer' },
      ],
    ];
    for (const [raw, target] of cases) expect(resolveSettingsTarget(raw), raw).toEqual(target);
    // Rows that stayed keep their links.
    expect(resolveSettingsTarget('appearance:text#size')).toEqual({
      group: 'appearance',
      section: 'text',
      anchor: 'size',
    });
    expect(resolveSettingsTarget('input:command-line#caret')).toEqual({
      group: 'input',
      section: 'command-line',
      anchor: 'caret',
    });
  });

  it('drops an empty section or anchor', () => {
    expect(resolveSettingsTarget('characters:#tracked')).toEqual({
      group: 'characters',
      anchor: 'tracked',
    });
    expect(resolveSettingsTarget('input:#')).toEqual({ group: 'input' });
  });

  it('opens General on anything it cannot read', () => {
    expect(resolveSettingsTarget('')).toEqual({ group: 'general' });
    expect(resolveSettingsTarget('hud')).toEqual({ group: 'general' });
    expect(resolveSettingsTarget('tick & chips')).toEqual({ group: 'general' });
    expect(resolveSettingsTarget('nowhere:macros#x')).toEqual({ group: 'general' });
  });
});

describe('settingsSubpage', () => {
  it('names the pages inside Logs', () => {
    expect(settingsSubpage(resolveSettingsTarget('general:logs'))).toBe('Search logs');
    expect(settingsSubpage(resolveSettingsTarget('logs'))).toBe('Search logs');
    expect(settingsSubpage(resolveSettingsTarget('logs:scene'))).toBe('Save a scene');
    expect(settingsSubpage({ group: 'logs' })).toBeNull();
  });

  it('is null for a group page and its sections', () => {
    expect(settingsSubpage({ group: 'general' })).toBeNull();
    expect(settingsSubpage({ group: 'general', section: 'updates' })).toBeNull();
    expect(settingsSubpage({ group: 'automation', section: 'logs' })).toBeNull();
  });
});

describe('a plugin page under Scripts', () => {
  it('keeps the case of the plugin name and titles the page with it', () => {
    const target = resolveSettingsTarget('scripts:Vitals_Alert');
    expect(target).toEqual({ group: 'scripts', section: 'Vitals_Alert' });
    expect(settingsSubpage(target)).toBe('Vitals_Alert');
    expect(formatSettingsTarget(target)).toBe('scripts:Vitals_Alert');
    expect(resolveSettingsTarget('Scripts:wait_full#Output')).toEqual({
      group: 'scripts',
      section: 'wait_full',
      anchor: 'output',
    });
  });

  it('leads the crumb back to the list, which is no page of its own', () => {
    // The crumb names the group and links to it bare.
    expect(settingsGroupLabel('scripts')).toBe('Scripts');
    expect(settingsSubpage({ group: 'scripts' })).toBeNull();
    expect(formatSettingsTarget({ group: 'scripts' })).toBe('scripts');
  });

  it('leaves the page for the list, another plugin or another group', () => {
    const page = { group: 'scripts', section: 'wait_full' } as const;
    // The crumb back to Scripts asks a dirty plugin page first.
    expect(leavesSettingsPage(page, { group: 'scripts' })).toBe(true);
    expect(leavesSettingsPage(page, { group: 'scripts', section: 'vitals_alert' })).toBe(true);
    expect(leavesSettingsPage(page, { group: 'automation' })).toBe(true);
    // A second press of the same link stays.
    expect(leavesSettingsPage(page, { group: 'scripts', section: 'wait_full' })).toBe(false);
    // A move inside a group with no page in it stays on the page, so
    // Automation asks about its kinds itself.
    expect(
      leavesSettingsPage(
        { group: 'automation', section: 'triggers' },
        { group: 'automation', section: 'aliases' },
      ),
    ).toBe(false);
  });

  it('never scrolls to the plugin', () => {
    expect(settingsScrollIds({ group: 'scripts', section: 'vitals_alert' })).toEqual([]);
    expect(settingsScrollIds({ group: 'scripts', anchor: 'console' })).toEqual(['console']);
  });
});

describe('formatSettingsTarget', () => {
  it('writes the string resolveSettingsTarget reads back', () => {
    const targets: SettingsTarget[] = [
      { group: 'general' },
      { group: 'automation', section: 'macros' },
      { group: 'automation', anchor: 'import' },
      { group: 'characters', section: 'Ilsabet', anchor: 'tracked' },
    ];
    for (const target of targets) {
      expect(resolveSettingsTarget(formatSettingsTarget(target))).toEqual(target);
    }
    expect(
      formatSettingsTarget({ group: 'characters', section: 'Ilsabet', anchor: 'tracked' }),
    ).toBe('characters:Ilsabet#tracked');
  });
});

describe('settingsScrollIds', () => {
  it('scrolls to the anchor first, then a section that names a place', () => {
    expect(settingsScrollIds({ group: 'appearance', section: 'text', anchor: 'size' })).toEqual([
      'size',
      'text',
    ]);
    expect(settingsScrollIds({ group: 'general', section: 'updates' })).toEqual(['updates']);
    expect(settingsScrollIds({ group: 'layout' })).toEqual([]);
  });

  it('never scrolls to a page inside a group', () => {
    expect(settingsScrollIds({ group: 'logs', section: 'search' })).toEqual([]);
  });

  it('never scrolls to an Automation kind or a Characters profile', () => {
    expect(settingsScrollIds({ group: 'automation', section: 'timers', anchor: 'tick' })).toEqual([
      'tick',
    ]);
    expect(settingsScrollIds({ group: 'automation', section: 'macros' })).toEqual([]);
    expect(
      settingsScrollIds({ group: 'characters', section: 'Ilsabet', anchor: 'tracked' }),
    ).toEqual(['tracked']);
  });
});
