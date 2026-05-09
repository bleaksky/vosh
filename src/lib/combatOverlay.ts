// Combat overlay store — Phase 13.
//
// Holds the user's toggle preferences for inline damage labels and
// averages, plus an auto-hide threshold (suppresses annotations
// above a chosen character level so veterans aren't pestered by
// hints they don't need). Persists to localStorage and broadcasts
// changes via a Tauri event so the Terminal in the main window
// picks up edits made in the Settings window without a relaunch.

import { emit, listen } from '@tauri-apps/api/event';

const STORAGE_KEY = 'mudclient.combat.overlay.v1';
const SYNC_EVENT = 'mudclient://combat-overlay-changed';

export interface CombatOverlaySettings {
  showLabels: boolean;
  showAverages: boolean;
  autoHide: boolean;
  /** When autoHide is on and the character's level >= this, both
   *  labels and averages are suppressed regardless of the explicit
   *  toggles. Default 30 since most players know the table by
   *  remort but newcomers grinding to ~30 still benefit. */
  autoHideAbove: number;
}

const DEFAULTS: CombatOverlaySettings = {
  showLabels: true,
  showAverages: true,
  autoHide: true,
  autoHideAbove: 30,
};

type Listener = () => void;

class CombatOverlayStore {
  private settings: CombatOverlaySettings = { ...DEFAULTS };
  private level: number | null = null;
  private listeners = new Set<Listener>();

  constructor() {
    this.load();
    // Subscribe to cross-window updates. Skip the emit on receive
    // (applyExternal) so we don't loop when our own emit echoes back
    // to this window.
    listen<CombatOverlaySettings>(SYNC_EVENT, (event) => {
      this.applyExternal(event.payload);
    }).catch(() => {
      // Tauri not available (tests, dev preview); ignore.
    });
  }

  private applyExternal(next: CombatOverlaySettings) {
    if (
      next.showLabels === this.settings.showLabels &&
      next.showAverages === this.settings.showAverages &&
      next.autoHide === this.settings.autoHide &&
      next.autoHideAbove === this.settings.autoHideAbove
    ) {
      return;
    }
    this.settings = { ...next };
    this.save();
    this.notify();
  }

  private load() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return;
      const parsed = JSON.parse(raw) as Partial<CombatOverlaySettings>;
      this.settings = {
        showLabels: parsed.showLabels ?? DEFAULTS.showLabels,
        showAverages: parsed.showAverages ?? DEFAULTS.showAverages,
        autoHide: parsed.autoHide ?? DEFAULTS.autoHide,
        autoHideAbove:
          typeof parsed.autoHideAbove === 'number' && parsed.autoHideAbove > 0
            ? Math.floor(parsed.autoHideAbove)
            : DEFAULTS.autoHideAbove,
      };
    } catch {
      // Bad JSON or storage unavailable; keep defaults.
    }
  }

  private save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.settings));
    } catch {
      // Storage full or private mode; persistence best-effort.
    }
  }

  getSettings(): CombatOverlaySettings {
    return { ...this.settings };
  }

  setSettings(next: Partial<CombatOverlaySettings>) {
    this.settings = {
      ...this.settings,
      ...next,
    };
    if (typeof next.autoHideAbove === 'number') {
      this.settings.autoHideAbove = Math.max(1, Math.floor(next.autoHideAbove));
    }
    this.save();
    this.notify();
    // Broadcast so the Terminal in the other window picks it up.
    emit(SYNC_EVENT, this.settings).catch(() => {
      // Tauri not available; persistence is enough.
    });
  }

  setLevel(level: number | null) {
    if (this.level === level) return;
    this.level = level;
    this.notify();
  }

  /// Effective enabled flags taking auto-hide into account. Used by
  /// the Terminal output decorator on every chunk.
  effective(): { showLabels: boolean; showAverages: boolean } {
    const suppressed =
      this.settings.autoHide &&
      this.level !== null &&
      this.level >= this.settings.autoHideAbove;
    return {
      showLabels: this.settings.showLabels && !suppressed,
      showAverages: this.settings.showAverages && !suppressed,
    };
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notify() {
    for (const l of this.listeners) l();
  }
}

export const combatOverlayStore = new CombatOverlayStore();
