// Shared affect helpers for the Char.Affects feed Aabahran sends.

export interface AffectModifier {
  location: string;
  modifier: number | string;
}

export function formatModifier(modifier: number | string): string {
  const n = typeof modifier === 'number' ? modifier : Number(modifier);
  if (!Number.isFinite(n)) return String(modifier);
  if (n > 0) return `+${n}`;
  return String(n);
}

/// Normalize a tracked-affect name for comparison. Case-insensitive,
/// whitespace collapsed, so "Blade Barrier" matches "blade  barrier".
export function normalizeAffectName(name: string): string {
  return name.toLowerCase().replace(/\s+/g, ' ').trim();
}
