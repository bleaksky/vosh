import { describe, expect, it } from 'vitest';
import { planSubmit, type SubmitContext } from './maskedInput';

// Made up values only. None of these is anyone's password.
const SECRET = 'Tr0ub4dor&3';

const typed = (patch: Partial<SubmitContext> = {}): SubmitContext => ({
  masked: false,
  quickKey: false,
  echoColor: null,
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
