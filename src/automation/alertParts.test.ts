import { describe, expect, it } from 'vitest';
import profileFull from '../../fixtures/config/profile.full.toml?raw';
import type { AlertParts } from '../ipc/automation';
import { ALERT_TONES } from '../stores/session/alertTones';
import {
  alertOrNone,
  normalizeAlert,
  withAlertPart,
  withAlertParts,
  type AlertPart,
} from './alertParts';
import { normalizeTrigger, triggerForSave } from './automationTriggers';

/** The keys and values of the table `head` names in the TOML, each a
 *  string or a switch, as the alert tables of the fixture hold them. */
function tomlTable(text: string, head: string): Record<string, unknown> {
  const lines = text.split('\n');
  const start = lines.indexOf(`[${head}]`);
  if (start < 0) throw new Error(`no [${head}] in the fixture`);
  const out: Record<string, unknown> = {};
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith('[')) break;
    const m = /^(\w+) = (?:"(.*)"|(true|false))$/.exec(line);
    if (m) out[m[1]] = m[2] ?? m[3] === 'true';
  }
  return out;
}

/** The default table, which rings nothing. */
const QUIET: AlertParts = { banner: false, background: true, words: false };

/** A visitor trigger with every part on. */
const VISITOR: AlertParts = {
  banner: true,
  sound: 'chime',
  attention: 'once',
  background: true,
  words: true,
};

describe('normalizeAlert', () => {
  it('reads the alert tables of profile.full.toml as Rust does', () => {
    expect(normalizeAlert(tomlTable(profileFull, 'triggers.alert'))).toEqual({
      banner: true,
      sound: 'chime',
      attention: 'until',
      background: false,
      words: true,
    });
    expect(normalizeAlert(tomlTable(profileFull, 'alerts.alert_low_health'))).toEqual({
      banner: false,
      sound: 'low',
      background: false,
      words: false,
    });
    expect(normalizeAlert(tomlTable(profileFull, 'alerts.alert_tells'))).toEqual({
      banner: true,
      sound: 'bell',
      attention: 'once',
      background: true,
      words: false,
    });
  });

  it('gives a missing key its default, as alert.rs does', () => {
    expect(normalizeAlert({})).toEqual(QUIET);
    expect(normalizeAlert({ banner: true })).toEqual({ ...QUIET, banner: true });
    expect(normalizeAlert({ sound: null, attention: null })).toEqual(QUIET);
  });

  it('reads an attention it does not know as none', () => {
    expect(normalizeAlert({ banner: true, attention: 'forever' })).toEqual({
      ...QUIET,
      banner: true,
    });
    expect(normalizeAlert({ attention: 3 })).toEqual(QUIET);
    expect(normalizeAlert({ attention: 'until' })).toEqual({ ...QUIET, attention: 'until' });
  });

  it('reads no alert where Rust fails the table', () => {
    expect(normalizeAlert(undefined)).toBeUndefined();
    expect(normalizeAlert(null)).toBeUndefined();
    expect(normalizeAlert('chime')).toBeUndefined();
    expect(normalizeAlert([VISITOR])).toBeUndefined();
    expect(normalizeAlert({ banner: 'yes' })).toBeUndefined();
    expect(normalizeAlert({ banner: true, background: null })).toBeUndefined();
    expect(normalizeAlert({ banner: true, words: 1 })).toBeUndefined();
    expect(normalizeAlert({ banner: true, sound: 5 })).toBeUndefined();
  });
});

describe('withAlertPart', () => {
  const parts: AlertPart[] = ['banner', 'sound', 'attention'];

  it('presses each part on from no alert, Sound on Chime and Bounce once', () => {
    expect(withAlertPart(undefined, 'banner', true)).toEqual({ ...QUIET, banner: true });
    expect(withAlertPart(undefined, 'sound', true)).toEqual({ ...QUIET, sound: 'chime' });
    expect(withAlertPart(undefined, 'attention', true)).toEqual({ ...QUIET, attention: 'once' });
    expect(withAlertPart(undefined, 'sound', true).sound).toBe(ALERT_TONES[0].value);
  });

  it('releases each part and leaves the others as they were', () => {
    expect(withAlertPart(VISITOR, 'banner', false)).toEqual({ ...VISITOR, banner: false });
    const { sound: _sound, ...noSound } = VISITOR;
    expect(withAlertPart(VISITOR, 'sound', false)).toEqual(noSound);
    const { attention: _attention, ...noBounce } = VISITOR;
    expect(withAlertPart(VISITOR, 'attention', false)).toEqual(noBounce);
  });

  it('keeps the tone and the length of a part that is on', () => {
    const bell = { ...VISITOR, sound: 'bell', attention: 'until' } as const;
    expect(withAlertPart(bell, 'sound', true)).toEqual(bell);
    expect(withAlertPart(bell, 'attention', true)).toEqual(bell);
  });

  it('comes back to the table it started from after a press and a release', () => {
    for (const part of parts) {
      expect(withAlertPart(withAlertPart(QUIET, part, true), part, false)).toEqual(QUIET);
    }
  });

  it('leaves the table it was given alone', () => {
    const before = { ...VISITOR };
    withAlertPart(VISITOR, 'sound', false);
    expect(VISITOR).toEqual(before);
  });
});

describe('withAlertParts', () => {
  it('starts from the default table, so a tone turns Sound on', () => {
    expect(withAlertParts(undefined, { sound: 'bell' })).toEqual({ ...QUIET, sound: 'bell' });
    expect(withAlertParts(undefined, { attention: 'until' })).toEqual({
      ...QUIET,
      attention: 'until',
    });
  });

  it('sets what it is given and leaves the rest, and the table it was given, alone', () => {
    const before = { ...VISITOR };
    expect(withAlertParts(VISITOR, { words: false, sound: 'low' })).toEqual({
      ...VISITOR,
      words: false,
      sound: 'low',
    });
    expect(VISITOR).toEqual(before);
  });
});

describe('alertOrNone', () => {
  it('drops the default table and keeps any other', () => {
    expect(alertOrNone(undefined)).toBeUndefined();
    expect(alertOrNone({ ...QUIET })).toBeUndefined();
    expect(alertOrNone(VISITOR)).toBe(VISITOR);
    // Nothing on, but you chose when it rings or what its banner shows.
    expect(alertOrNone({ ...QUIET, background: false })).toEqual({ ...QUIET, background: false });
    expect(alertOrNone({ ...QUIET, words: true })).toEqual({ ...QUIET, words: true });
  });
});

describe('a trigger you never touch', () => {
  const stored = {
    name: 'visitor',
    pattern: '^(\\w+) walks in\\.$',
    patterns: [{ pattern: '^(\\w+) walks in\\.$', enabled: true }],
    priority: 5,
    enabled: true,
    actions: [],
  };

  it('saves its alert as it came', () => {
    const t = triggerForSave(normalizeTrigger({ ...stored, alert: VISITOR }));
    expect(t.alert).toEqual(VISITOR);
    const quiet = triggerForSave(normalizeTrigger({ ...stored, alert: QUIET }));
    expect(quiet.alert).toEqual(QUIET);
  });

  it('saves no alert when it had none', () => {
    expect('alert' in triggerForSave(normalizeTrigger(stored))).toBe(false);
    expect('alert' in normalizeTrigger({ ...stored, alert: { banner: 'yes' } })).toBe(false);
  });
});
