// Shared affect helpers. Both the status-bar tracked pills and the
// side-pane full list read from the same Char.Affects payload shape
// Aabahran sends.

export interface Affect {
  name: string;
  /// Hours remaining. -1 means permanent. The server sometimes sends it
  /// as a string; coerce on read.
  duration?: number;
  /// Human-readable hint when the server provides one.
  description?: string;
  /// Same name some servers use for description.
  desc?: string;
  /// Stack count when the server tracks it.
  stacks?: number;
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
