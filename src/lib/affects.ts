// Shared affect helpers for the Char.Affects feed Aabahran sends.

export interface AffectModifier {
  location: string;
  modifier: number | string;
}

/// Normalize a tracked-affect name for comparison. Case-insensitive,
/// whitespace collapsed, so "Blade Barrier" matches "blade  barrier".
export function normalizeAffectName(name: string): string {
  return name.toLowerCase().replace(/\s+/g, ' ').trim();
}
