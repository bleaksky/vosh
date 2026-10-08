import { describe, expect, it } from 'vitest';
import { OTHER, SAVED, worldChoice } from './worldChoice';

// The World select in General › Connection.

const FORSAKEN = 'world:theforsakenlands.com';
const labels = (target: Parameters<typeof worldChoice>[0]) =>
  worldChoice(target).options.map((o) => [o.value, o.label]);

describe('the World select', () => {
  it('shows a known world on its own port as that world', () => {
    const target = { host: 'play.theforsakenlands.com', port: 1848, tls: false };
    expect(worldChoice(target).value).toBe(FORSAKEN);
    expect(labels(target)).toEqual([
      [FORSAKEN, 'The Forsaken Lands'],
      [OTHER, 'Other…'],
    ]);
  });

  it('labels a known world on another port as its row does, apart from the world', () => {
    const target = { host: 'play.theforsakenlands.com', port: 1825, tls: false };
    expect(worldChoice(target).value).toBe(SAVED);
    expect(labels(target)).toEqual([
      [FORSAKEN, 'The Forsaken Lands'],
      [SAVED, 'The Forsaken Lands 1825'],
      [OTHER, 'Other…'],
    ]);
  });

  it('shows any other host as typed', () => {
    const target = { host: 'mud.example.org', port: 4000, tls: true };
    expect(worldChoice(target).value).toBe(SAVED);
    expect(labels(target)).toEqual([
      [FORSAKEN, 'The Forsaken Lands'],
      [SAVED, 'mud.example.org'],
      [OTHER, 'Other…'],
    ]);
  });
});
