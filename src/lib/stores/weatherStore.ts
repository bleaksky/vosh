import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onState } from '../session';
import { asNumber, asText, createStore } from './store';

// The weather where you stand, from Room.Weather. Aabahran sends
// `{sky, temp, unit, region}` each time a prompt would print, what the
// prompt's %W, %w and %G show. The sky reads indoors when you are not
// outside, and the unit follows your celsius setting. World.Time
// carries the sky of the world at large (worldStore). The prompt
// engine in the backend keeps its own copy to draw your prompt, so
// nothing here shows it yet. The store is ready for a pane that does.

export interface RoomWeather {
  /** cloudless, cloudy, rainy, snowing, lightning and the like, or
   *  indoors. */
  sky: string | null;
  /** In `unit`. */
  temp: number | null;
  unit: 'C' | 'F' | null;
  /** The climate region's name, like Coastal North. */
  region: string | null;
}

/** Parse a Room.Weather payload. null when it is not an object. */
export function parseRoomWeather(data: unknown): RoomWeather | null {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
  const d = data as Record<string, unknown>;
  return {
    sky: asText(d.sky),
    temp: asNumber(d.temp),
    unit: d.unit === 'C' || d.unit === 'F' ? d.unit : null,
    region: asText(d.region),
  };
}

function sameWeather(a: RoomWeather, b: RoomWeather): boolean {
  return a.sky === b.sky && a.temp === b.temp && a.unit === b.unit && a.region === b.region;
}

const store = createStore<RoomWeather | null>(null);
let started = false;

export function startWeatherStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Room.Weather', (data) => {
    const next = parseRoomWeather(data);
    if (!next) return;
    const prev = store.get();
    // It rides every prompt, so skip the ones that repeat.
    if (prev && sameWeather(prev, next)) return;
    store.set(next);
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') store.set(null);
  });
}

export function getRoomWeather(): RoomWeather | null {
  return store.get();
}

export function subscribeRoomWeather(cb: () => void): () => void {
  startWeatherStore();
  return store.subscribe(cb);
}

/** The weather where you stand, or null until the game sends it. */
export function useRoomWeather(): RoomWeather | null {
  return useSyncExternalStore(subscribeRoomWeather, getRoomWeather);
}
