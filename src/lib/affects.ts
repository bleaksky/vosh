// Shared affect helpers. Both the status-bar tracked pills and the
// side-pane full list read from the same Char.Affects payload shape
// Aabahran sends.

export interface Affect {
  name: string;
  /// Hours remaining. -1 means permanent. The server sometimes sends it
  /// as a string; coerce on read.
  duration?: number;
  /// Stat the modifier applies to (e.g. "saves", "hitroll", "str").
  /// Aabahran sends one entry per (affect, location, modifier); bless
  /// shows up as several rows with the same name but different
  /// locations.
  location?: string;
  /// Numeric magnitude of the modifier on `location`. Negative for
  /// improvements on saves; positive for stat boosts; etc.
  modifier?: number | string;
  /// Human-readable hint when the server provides one.
  description?: string;
  /// Same name some servers use for description.
  desc?: string;
  /// Stack count when the server tracks it.
  stacks?: number;
}

export interface AffectModifier {
  location: string;
  modifier: number | string;
}

export interface GroupedAffect {
  name: string;
  duration?: number;
  description?: string;
  modifiers: AffectModifier[];
}

/// Aabahran sends one Char.Affects entry per (affect, modifier) pair.
/// Group them so multi-modifier spells like bless appear as a single
/// row with several modifiers attached.
export function groupAffects(raw: Affect[]): GroupedAffect[] {
  const order: string[] = [];
  const map = new Map<string, GroupedAffect>();
  for (const aff of raw) {
    if (!aff?.name) continue;
    let group = map.get(aff.name);
    if (!group) {
      const dur =
        typeof aff.duration === 'number'
          ? aff.duration
          : aff.duration !== undefined
            ? Number(aff.duration)
            : undefined;
      group = {
        name: aff.name,
        modifiers: [],
      };
      if (dur !== undefined && Number.isFinite(dur)) group.duration = dur;
      const desc = affectDescription(aff);
      if (desc) group.description = desc;
      map.set(aff.name, group);
      order.push(aff.name);
    }
    if (aff.location && aff.location !== 'none') {
      group.modifiers.push({
        location: aff.location,
        modifier: aff.modifier ?? 0,
      });
    }
  }
  return order.map((name) => map.get(name)!);
}

export function formatModifier(modifier: number | string): string {
  const n = typeof modifier === 'number' ? modifier : Number(modifier);
  if (!Number.isFinite(n)) return String(modifier);
  if (n > 0) return `+${n}`;
  return String(n);
}

/// Tone the duration cell to convey urgency, mirroring the tintin
/// affects panel's color ladder. Permanent gets the cool blue.
export function colorForDuration(duration: number | undefined): string {
  if (duration === undefined) return '#a4a7a4';
  if (duration < 0) return '#7aa89f'; // permanent / cyan
  if (duration <= 2) return '#e46876'; // very urgent — red
  if (duration <= 5) return '#e0823c'; // orange
  if (duration <= 10) return '#e6c384'; // bright yellow
  if (duration <= 20) return '#d4c441'; // yellow
  if (duration <= 40) return '#a8b87f'; // pale green
  return '#87a987'; // healthy green
}

export function formatDuration(duration: number | undefined): string {
  if (duration === undefined) return '?';
  if (duration < 0) return 'prm';
  return `${duration}h`;
}

export function affectDescription(affect: Affect): string | null {
  return affect.description ?? affect.desc ?? null;
}

/// Normalize a tracked-affect name for comparison. Case-insensitive,
/// whitespace collapsed, so "Blade Barrier" matches "blade  barrier".
export function normalizeAffectName(name: string): string {
  return name.toLowerCase().replace(/\s+/g, ' ').trim();
}
