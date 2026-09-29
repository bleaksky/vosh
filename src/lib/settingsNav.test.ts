import { describe, expect, it } from 'vitest';
import {
  formatSettingsTarget,
  resolveSettingsTarget,
  settingsScrollIds,
  SETTINGS_GROUPS,
  type SettingsTarget,
} from './settingsNav';

describe('resolveSettingsTarget', () => {
  it('maps every tab id the old window used', () => {
    const cases: [string, SettingsTarget][] = [
      ['general', { group: 'general' }],
      ['themes', { group: 'appearance', section: 'theme' }],
      ['typography', { group: 'appearance', section: 'text' }],
      ['vitals', { group: 'layout' }],
      ['tick', { group: 'automation', section: 'timers', anchor: 'tick' }],
      ['panels', { group: 'characters', anchor: 'layout' }],
      ['profiles', { group: 'characters' }],
      ['loadouts', { group: 'automation', section: 'loadouts' }],
      ['triggers', { group: 'automation', section: 'triggers' }],
      ['aliases', { group: 'automation', section: 'aliases' }],
      ['macros', { group: 'automation', section: 'macros' }],
      ['timers', { group: 'automation', section: 'timers' }],
      ['import', { group: 'automation', anchor: 'import' }],
      ['logs', { group: 'general', section: 'logs' }],
    ];
    for (const [raw, target] of cases) expect(resolveSettingsTarget(raw)).toEqual(target);
  });

  it('reads a bare group in every one of the six', () => {
    for (const { id } of SETTINGS_GROUPS) expect(resolveSettingsTarget(id)).toEqual({ group: id });
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
    expect(resolveSettingsTarget('characters:Erelei#tracked')).toEqual({
      group: 'characters',
      section: 'Erelei',
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

describe('formatSettingsTarget', () => {
  it('writes the string resolveSettingsTarget reads back', () => {
    const targets: SettingsTarget[] = [
      { group: 'general' },
      { group: 'automation', section: 'macros' },
      { group: 'automation', anchor: 'import' },
      { group: 'characters', section: 'Erelei', anchor: 'tracked' },
    ];
    for (const target of targets) {
      expect(resolveSettingsTarget(formatSettingsTarget(target))).toEqual(target);
    }
    expect(
      formatSettingsTarget({ group: 'characters', section: 'Erelei', anchor: 'tracked' }),
    ).toBe('characters:Erelei#tracked');
  });
});

describe('settingsScrollIds', () => {
  it('scrolls to the anchor first, then a section that names a place', () => {
    expect(settingsScrollIds({ group: 'appearance', section: 'text', anchor: 'size' })).toEqual([
      'size',
      'text',
    ]);
    expect(settingsScrollIds({ group: 'general', section: 'logs' })).toEqual(['logs']);
    expect(settingsScrollIds({ group: 'layout' })).toEqual([]);
  });

  it('never scrolls to an Automation kind or a Characters profile', () => {
    expect(settingsScrollIds({ group: 'automation', section: 'timers', anchor: 'tick' })).toEqual([
      'tick',
    ]);
    expect(settingsScrollIds({ group: 'automation', section: 'macros' })).toEqual([]);
    expect(
      settingsScrollIds({ group: 'characters', section: 'Erelei', anchor: 'tracked' }),
    ).toEqual(['tracked']);
  });
});
