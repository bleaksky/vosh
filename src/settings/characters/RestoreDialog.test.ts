import { describe, expect, it } from 'vitest';
import { backupTime } from './RestoreDialog';

// How Restore from a backup… names when each backup was set aside.

describe('backupTime', () => {
  const now = new Date(2026, 9, 27, 14, 30).getTime();
  const at = (day: number, hour: number, minute: number) =>
    new Date(2026, 9, day, hour, minute).getTime();

  it('says today and yesterday by name', () => {
    expect(backupTime(at(27, 9, 41), now)).toMatch(/^Today at 9:41/);
    expect(backupTime(at(27, 0, 5), now)).toMatch(/^Today at /);
    expect(backupTime(at(26, 23, 59), now)).toMatch(/^Yesterday at /);
  });

  it('names the day for anything older', () => {
    expect(backupTime(at(24, 9, 8), now)).toMatch(/^Oct 24 at /);
  });
});
