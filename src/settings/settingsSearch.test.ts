import { describe, expect, it } from 'vitest';
import { formatSettingsTarget, resolveSettingsTarget } from '../lib/settingsNav';
import { SETTINGS_ROWS, searchSettingsRows, settingsRowKey } from './settingsSearch';

const mac = { pathB: false, mac: true };
const labels = (query: string, env = mac) => searchSettingsRows(query, env).map((r) => r.label);

describe('SETTINGS_ROWS', () => {
  it('gives every row a unique target that survives the deep link round trip', () => {
    const keys = SETTINGS_ROWS.map(settingsRowKey);
    expect(new Set(keys).size).toBe(keys.length);
    for (const row of SETTINGS_ROWS) {
      expect(resolveSettingsTarget(formatSettingsTarget(row.target))).toEqual(row.target);
    }
  });

  it('covers all seven groups', () => {
    const groups = new Set(SETTINGS_ROWS.map((r) => r.target.group));
    expect([...groups]).toEqual([
      'general',
      'appearance',
      'layout',
      'input',
      'automation',
      'scripts',
      'characters',
    ]);
  });
});

describe('searchSettingsRows', () => {
  it('finds nothing for an empty query', () => {
    expect(labels('')).toEqual([]);
    expect(labels('   ')).toEqual([]);
  });

  it('ranks a label that starts with the query first', () => {
    expect(labels('line')[0]).toBe('Line height');
    expect(labels('font')[0]).toBe('Font');
    expect(labels('Tracked')[0]).toBe('Tracked affects');
  });

  it('matches descriptions and keywords', () => {
    expect(labels('panel font')[0]).toBe('Panel font');
    expect(labels('status line typeface')).toEqual(['Panel font']);
    expect(labels('panel size')[0]).toBe('Panel size');
    expect(labels('status line bigger')).toEqual(['Panel size']);
    expect(searchSettingsRows('panel size', mac)[0].target).toEqual({
      group: 'appearance',
      section: 'panel-text',
      anchor: 'panel-size',
    });
    expect(labels('ghostty')).toEqual(['Import a theme']);
    expect(labels('cursor')).toEqual(['Caret shape']);
    expect(labels('missing')).toContain('Tracked affects');
  });

  it('finds where the session in front connects', () => {
    expect(labels('session port')).toEqual(['World', 'Host and port']);
    expect(labels('session tls')).toEqual(['Use TLS']);
  });

  it('needs every word, in any field', () => {
    expect(labels('dark theme')[0]).toBe('Dark theme');
    expect(labels('import')).toEqual([
      'Import a theme',
      'Import from another client',
      'Import a profile',
    ]);
    expect(labels('toml')).toEqual(['Import a profile']);
  });

  it('finds a group by its name', () => {
    const rows = searchSettingsRows('automation', mac);
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.every((r) => r.target.group === 'automation')).toBe(true);
  });

  it('finds the Theme row by the themes that left Vosh', () => {
    for (const name of ['vellum', 'one dark', 'everforest light']) {
      expect(labels(name)[0], name).toBe('Theme');
    }
  });

  it('ignores case and accents', () => {
    expect(labels('LINE HEIGHT')).toEqual(['Line height']);
    expect(labels('thème')[0]).toBe('Theme');
  });

  it('finds Plugins and Console on the Scripts list by lua', () => {
    expect(labels('lua')).toEqual(['Plugins', 'Console']);
    expect(labels('install plugin')).toEqual(['Plugins']);
    expect(labels('print')).toEqual(['Console']);
    expect(labels('scripts')).toEqual(['Plugins', 'Console']);
    expect(searchSettingsRows('lua', mac).map(settingsRowKey)).toEqual([
      'scripts#plugins',
      'scripts#console',
    ]);
  });

  it('finds Macros, Presets and the preset by numpad', () => {
    expect(labels('numpad')).toEqual(['Numpad movement', 'Macros', 'Presets']);
    expect(labels('walk keys')).toEqual(['Presets']);
  });

  it('finds Triggers by the parts of an alert', () => {
    expect(labels('bounce')).toEqual(['Triggers']);
    expect(labels('flash')).toContain('Triggers');
    expect(labels('alert banner')).toEqual(['Triggers', 'Presets']);
    expect(labels('notification')).toEqual(['Triggers', 'Presets']);
    expect(labels('alert tone')).toEqual(['Triggers']);
    expect(labels('chime')).toEqual(['Triggers']);
  });

  it('finds Presets by the alert presets', () => {
    expect(labels('tells')).toContain('Presets');
    expect(labels('attacked')).toEqual(['Presets']);
    expect(labels('health')).toContain('Presets');
    expect(labels('connection')).toContain('Presets');
  });

  it('finds each preset on its card, by its colors and Reset to preset', () => {
    expect(labels('herb')).toEqual(['Herb labels', 'Potion labels']);
    expect(searchSettingsRows('disarms', mac)[0].target).toEqual({
      group: 'automation',
      section: 'presets',
      anchor: 'presets:disarm_buff_fade',
    });
    const swatches = labels('swatch');
    expect(swatches).toContain('Room, time and weather colors');
    expect(swatches).not.toContain('Numpad movement');
    expect(swatches).not.toContain('Tells you send');
    expect(labels('reset to preset')).toContain('Numpad movement');
    expect(labels('your changes')).toContain('Disarms and fading buffs');
  });

  it('shows loadouts only in loadout mode', () => {
    expect(labels('loadouts')).not.toContain('Loadouts');
    expect(labels('loadouts', { pathB: true, mac: true })).toContain('Loadouts');
  });

  it('finds the Advanced appearance rows', () => {
    expect(labels('font stack')).toEqual(['Font stack']);
    expect(labels('fallback')).toEqual(['Font stack']);
    expect(labels('palette')[0]).toBe('Base palette');
  });

  it('finds Collapse repeated lines by what it does to spam', () => {
    expect(labels('collapse')).toEqual(['Collapse repeated lines', 'In a fight', 'Attack lines']);
    expect(labels('spam')).toEqual(['Collapse repeated lines']);
    expect(labels('duplicate lines')).toEqual(['Collapse repeated lines']);
    expect(labels('repeated')[0]).toBe('Collapse repeated lines');
  });

  it('finds In a fight and Attack lines under Collapse repeated lines', () => {
    expect(labels('fight')[0]).toBe('In a fight');
    expect(labels('combat').slice(0, 2)).toEqual(['In a fight', 'Attack lines']);
    expect(labels('attack')[0]).toBe('Attack lines');
    expect(labels('damage')).toEqual(['Damage to you', 'Your damage verbs', 'Attack lines']);
    const target = (label: string) => SETTINGS_ROWS.find((r) => r.label === label)?.target;
    expect(target('In a fight')).toEqual({
      group: 'appearance',
      section: 'text',
      anchor: 'collapse-fights',
    });
    expect(target('Attack lines')).toEqual({
      group: 'appearance',
      section: 'text',
      anchor: 'collapse-attacks',
    });
  });

  it('finds the Input rows, Advanced ones included', () => {
    expect(labels('paste')).toEqual(['Wait between pasted lines']);
    expect(labels('sent command')[0]).toBe('Sent command color');
    const paste = searchSettingsRows('paste', mac)[0];
    expect(paste.target).toEqual({ group: 'input', section: 'advanced', anchor: 'paste-delay' });
  });

  it('finds the Prompt section rows (P12)', () => {
    const target = (label: string) => SETTINGS_ROWS.find((r) => r.label === label)?.target ?? null;
    expect(target("Your game's prompt")).toEqual({
      group: 'input',
      section: 'prompt',
      anchor: 'prompt-game',
    });
    expect(target('Draw your own prompt')).toEqual({ group: 'input', section: 'prompt' });
    expect(target('Where your prompt shows')).toEqual({
      group: 'input',
      section: 'prompt',
      anchor: 'prompt-show',
    });
    expect(labels('prompt template')).toEqual(['Draw your own prompt']);
    expect(labels('customize')).toContain('Draw your own prompt');
    // The fight prompt reads in the same block.
    expect(labels('fight prompt')[0]).toBe("Your game's prompt");
    expect(labels('fprompt')).toEqual(["Your game's prompt"]);
    expect(labels('prompt codes')[0]).toBe("Your game's prompt");
    // Nothing about your prompt is left under Advanced.
    expect(
      SETTINGS_ROWS.filter(
        (r) => r.target.group === 'input' && r.target.section === 'advanced',
      ).map((r) => r.label),
    ).toEqual(['Wait between pasted lines']);
  });

  it('finds the Layout rows', () => {
    expect(labels('vitals')).toContain('Style');
    expect(labels('one line')).toEqual(['Style']);
    expect(labels('gauges')).toEqual(['Style']);
    expect(labels('status line')).toContain('Show your vitals in');
    const style = searchSettingsRows('pips', mac)[0];
    expect(style.target).toEqual({ group: 'layout', section: 'vitals', anchor: 'style' });
    expect(labels('panel width')[0]).toBe('Width');
    expect(labels('divider')).toEqual(['Divider color']);
    const divider = searchSettingsRows('divider', mac)[0];
    expect(divider.target).toEqual({ group: 'layout', section: 'split', anchor: 'divider-color' });
  });

  it('finds the Vitals rows the VitalsOptions board adds', () => {
    expect(labels('vitals')).toEqual(
      expect.arrayContaining(['Style', 'Values', 'Meter', 'Warn before you run low']),
    );
    expect(labels('percent')[0]).toBe('Values');
    expect(labels('meter')[0]).toBe('Meter');
    expect(labels('bar')).toContain('Meter');
    expect(labels('warn')[0]).toBe('Warn before you run low');
    // Running out at turns hours yellow, which holds both words too.
    expect(labels('run low')).toEqual(['Warn before you run low', 'Running out at']);
    const targets = ['values', 'meter', 'warn-low'].map(
      (anchor) => SETTINGS_ROWS.find((r) => r.target.anchor === anchor)?.target,
    );
    expect(targets).toEqual([
      { group: 'layout', section: 'customize-vitals', anchor: 'values' },
      { group: 'layout', section: 'customize-vitals', anchor: 'meter' },
      { group: 'layout', section: 'customize-vitals', anchor: 'warn-low' },
    ]);
  });

  it('finds Customize vitals by its list, its colors and your opponent', () => {
    expect(labels('order')[0]).toBe('Vitals and their order');
    expect(labels('vitals color')[0]).toBe('Vitals and their order');
    expect(labels('opponent')).toContain('Your opponent');
    expect(labels('vitals colors')).toEqual(['Vitals and their order']);
    expect(labels('ledger')).toEqual(['Style']);
    expect(labels('vitals text')).toContain('Style');
    const order = SETTINGS_ROWS.find((r) => r.label === 'Vitals and their order');
    expect(order?.target).toEqual({
      group: 'layout',
      section: 'customize-vitals',
      anchor: 'vitals-order',
    });
  });

  it('finds the affects style and marker under Layout', () => {
    const [style] = searchSettingsRows('grouped chips', mac);
    expect(style.label).toBe('Style');
    expect(settingsRowKey(style)).toBe('layout:affects#affects-style');
    expect(labels('countdown')).toContain('Style');
    expect(labels('timers first')).toEqual(['Style']);
    const [marker] = searchSettingsRows('affects dot', mac);
    expect(marker.label).toBe('Marker');
    expect(settingsRowKey(marker)).toBe('layout:affects#affects-marker');
    expect(labels('square')).toContain('Marker');
    expect(labels('plus minus')).toContain('Marker');
    const [tint] = searchSettingsRows('tint recast', mac);
    expect(tint.label).toBe('Tint what to recast');
    expect(settingsRowKey(tint)).toBe('layout:affects#affects-tint');
    expect(labels('draining chips')).toEqual(['Style']);
  });

  it('finds when affects warn and turn red under Layout', () => {
    const [runningOut] = searchSettingsRows('running out', mac);
    expect(runningOut.label).toBe('Running out at');
    expect(settingsRowKey(runningOut)).toBe('layout:affects#affects-running-out');
    expect(labels('affects threshold')).toEqual(['Running out at', 'Almost gone at']);
    expect(labels('affects warn')[0]).toBe('Running out at');
    const [almostGone] = searchSettingsRows('almost gone', mac);
    expect(almostGone.label).toBe('Almost gone at');
    expect(settingsRowKey(almostGone)).toBe('layout:affects#affects-almost-gone');
    expect(labels('affects red')).toContain('Almost gone at');
  });

  it('finds the tick and time style under Layout', () => {
    const [row] = searchSettingsRows('chip style', mac);
    expect(row.label).toBe('Tick and time');
    expect(settingsRowKey(row)).toBe('layout:status#tick-time');
    expect(labels('status line')).toContain('Tick and time');
    expect(labels('icon')).toContain('Tick and time');
    expect(labels('moons')).toContain('Tick and time');
  });

  it('finds which way the tick counts under Layout', () => {
    const [row] = searchSettingsRows('tick count', mac);
    expect(row.label).toBe('Tick counts');
    expect(settingsRowKey(row)).toBe('layout:status#tick-counts');
    // The Countdown affects style matches too, after the tick.
    expect(labels('countdown')).toEqual(['Tick counts', 'Style']);
    expect(labels('below zero')).toEqual(['Tick counts']);
    expect(labels('tick')).toEqual(expect.arrayContaining(['Tick and time', 'Tick counts']));
  });

  it('hides GPU rendering on macOS', () => {
    expect(labels('gpu')).toEqual([]);
    expect(labels('gpu', { pathB: false, mac: false })).toEqual(['GPU rendering']);
  });

  it('opens the GPU row inside General Advanced', () => {
    const [row] = searchSettingsRows('gpu', { pathB: false, mac: false });
    expect(row.target).toEqual({ group: 'general', section: 'advanced', anchor: 'gpu' });
  });

  it('finds the log view by the old tab id target', () => {
    const [row] = searchSettingsRows('search logs', mac);
    expect(row.label).toBe('Search logs');
    expect(row.target).toEqual(resolveSettingsTarget('logs'));
    expect(labels('saved logs')[0]).toBe('Session logs');
    expect(labels('saved sessions')[0]).toBe('Session logs');
  });
});
