import { describe, expect, it } from 'vitest';
import echoMarks from '../../fixtures/input/echo-marks.json';
import { echoMarkOptionsOf, normalizeUiConfig, type RawUiConfig } from '../ipc/uiConfig';
import {
  commandEcho,
  DEFAULT_ECHO_MARK,
  draftAfterMaskChange,
  echoMark,
  isMasked,
  keepsLastCommand,
  macroEcho,
  planSubmit,
  type SubmitContext,
} from './maskedInput';

// Made up values only. None of these is anyone's password.
const SECRET = 'Tr0ub4dor&3';

const typed = (patch: Partial<SubmitContext> = {}): SubmitContext => ({
  masked: false,
  quickKey: false,
  echoColor: null,
  echoMark: '',
  echoDim: false,
  ...patch,
});

describe('a line submitted from the masked password field', () => {
  const plan = planSubmit(SECRET, typed({ masked: true, echoColor: '#a0c4ff' }));

  it('echoes only a line break to the terminal and the native renderer', () => {
    expect(plan.echo).toBe('\r\n');
  });

  it('stays out of command history and Up arrow recall', () => {
    expect(plan.remember).toBe(false);
  });

  it('goes to the server through the masked send, past the input pipeline', () => {
    expect(plan.masked).toBe(true);
  });

  it('is never run as a frontend command', () => {
    const local = planSubmit('#nativesurface on', typed({ masked: true }));
    expect(local.local).toBe(false);
    expect(local.masked).toBe(true);
  });

  it('keeps a quick key name out of the echo too', () => {
    expect(planSubmit('gg', typed({ masked: true, quickKey: true })).echo).toBe('\r\n');
  });
});

describe('a line submitted from the command row', () => {
  it('echoes in the echo color, joins history, and runs through the pipeline', () => {
    const plan = planSubmit('look', typed({ echoColor: '#102030' }));
    expect(plan.echo).toBe('\x1b[38;2;16;32;48mlook\x1b[0m\r\n');
    expect(plan.remember).toBe(true);
    expect(plan.masked).toBe(false);
    expect(plan.local).toBe(false);
  });

  it('leaves the echo of a quick key to the backend', () => {
    expect(planSubmit('gg', typed({ quickKey: true })).echo).toBeNull();
  });

  it('keeps a bare Enter out of history', () => {
    expect(planSubmit('', typed()).remember).toBe(false);
  });

  it('runs #nativesurface in the frontend', () => {
    expect(planSubmit('#nativesurface off', typed()).local).toBe(true);
  });
});

describe('Keep last command', () => {
  it('never leaves a line from the masked field selected in the input', () => {
    expect(keepsLastCommand(true, SECRET, true)).toBe(false);
  });

  it('keeps a typed command selected when the setting is on', () => {
    expect(keepsLastCommand(true, 'look', false)).toBe(true);
    expect(keepsLastCommand(false, 'look', false)).toBe(false);
    expect(keepsLastCommand(true, '', false)).toBe(false);
  });
});

describe('a macro pressed at a password prompt', () => {
  const macro = (patch: Partial<SubmitContext & { enabled: boolean }> = {}) => ({
    ...typed(),
    enabled: true,
    ...patch,
  });

  it('echoes nothing while the field is masked', () => {
    expect(macroEcho('stand', macro({ masked: true }))).toBeNull();
  });

  it('echoes like a typed command otherwise', () => {
    expect(macroEcho('stand', macro())).toBe('stand\r\n');
    expect(macroEcho('stand', macro({ enabled: false }))).toBeNull();
    expect(macroEcho('gg', macro({ quickKey: true }))).toBeNull();
  });
});

describe('Mark your commands', () => {
  const marked = (patch: Partial<SubmitContext> = {}) =>
    typed({ echoMark: DEFAULT_ECHO_MARK, ...patch });

  it('builds each echo with the bytes the quick key echo uses', () => {
    for (const { about, ui, command, echo } of echoMarks.cases) {
      const config = normalizeUiConfig(ui as RawUiConfig);
      const options = echoMarkOptionsOf(config);
      const built = commandEcho(command, config.input_echo_color, echoMark(options), options.dim);
      expect(built, about).toBe(`${echo}\r\n`);
    }
  });

  it('draws a grey single width chevron and a space in the theme bright black by default', () => {
    expect(DEFAULT_ECHO_MARK).toBe('\x1b[90m\u203a \x1b[0m');
  });

  it('puts the mark before a typed command and a macro command alike', () => {
    expect(planSubmit('look', marked()).echo).toBe(`${DEFAULT_ECHO_MARK}look\r\n`);
    expect(macroEcho('stand', { ...marked({ echoColor: '#ff8800' }), enabled: true })).toBe(
      `${DEFAULT_ECHO_MARK}\x1b[38;2;255;136;0mstand\x1b[0m\r\n`,
    );
    expect(planSubmit('look', marked({ echoDim: true })).echo).toBe(
      `${DEFAULT_ECHO_MARK}\x1b[2mlook\x1b[0m\r\n`,
    );
  });

  it('echoes the mark alone for a bare Enter, and an empty line with the mark off', () => {
    expect(planSubmit('', marked()).echo).toBe(`${DEFAULT_ECHO_MARK}\r\n`);
    expect(planSubmit('', typed()).echo).toBe('\r\n');
  });

  it('leaves a password and a quick key alone', () => {
    expect(planSubmit(SECRET, marked({ masked: true })).echo).toBe('\r\n');
    expect(planSubmit('gg', marked({ quickKey: true })).echo).toBeNull();
    expect(macroEcho('stand', { ...marked({ masked: true }), enabled: true })).toBeNull();
  });

  it('echoes bare with the mark off', () => {
    expect(planSubmit('look', typed()).echo).toBe('look\r\n');
  });
});

describe('Enter pressed before the input row catches up with the server', () => {
  // The input-mode event lands before React renders the row again, so a
  // key handler holds the draft of the last render while the newest
  // event already says something else.

  it('keeps a password masked when the server hands echo back before the row unmasks', () => {
    // WONT ECHO, or the input-mode false a disconnect sends, arrived
    // while the masked field still holds the password as its draft.
    const masked = isMasked(true, false);
    expect(masked).toBe(true);
    const plan = planSubmit(SECRET, typed({ masked, echoColor: '#a0c4ff' }));
    expect(plan.echo).toBe('\r\n');
    expect(plan.remember).toBe(false);
    expect(plan.masked).toBe(true);
    expect(keepsLastCommand(true, SECRET, masked)).toBe(false);
  });

  it('masks a line sent after the server takes echo but before the row masks', () => {
    expect(isMasked(false, true)).toBe(true);
  });

  it('treats the row as plain once both agree', () => {
    expect(isMasked(false, false)).toBe(false);
    expect(isMasked(true, true)).toBe(true);
  });
});

describe('the draft when the input row masks or unmasks', () => {
  it('drops what you typed at the password prompt when the server hands echo back', () => {
    // A server that gives up on the prompt sends WONT ECHO while you type.
    // The half typed password must not show in the unmasked row, where
    // Enter would echo it and put it in history.
    expect(draftAfterMaskChange(true, false, SECRET)).toBe('');
  });

  it('opens the password prompt empty', () => {
    // With Keep last command on, the account name you just sent stays in
    // the row. It must not end up in front of the password.
    expect(draftAfterMaskChange(false, true, 'wanderer')).toBe('');
  });

  it('keeps the draft when nothing changes, as on a disconnect outside a prompt', () => {
    expect(draftAfterMaskChange(false, false, 'look')).toBe('look');
    expect(draftAfterMaskChange(true, true, 'Tr0')).toBe('Tr0');
  });
});
