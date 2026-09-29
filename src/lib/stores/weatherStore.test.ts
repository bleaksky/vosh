import { describe, expect, it } from 'vitest';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { parseRoomWeather } from './weatherStore';

const fixture = (name: string) => parseRoomWeather(aabahranPacket(name).data);

describe('parseRoomWeather', () => {
  it('reads the weather outdoors', () => {
    expect(fixture('room-weather.gmcp')).toEqual({
      sky: 'rainy',
      temp: 60,
      unit: 'F',
      region: 'Coastal North',
    });
  });

  it('reads the weather indoors in Celsius', () => {
    expect(fixture('room-weather-indoors.gmcp')).toEqual({
      sky: 'indoors',
      temp: 18,
      unit: 'C',
      region: 'Temperate',
    });
  });

  it('keeps below freezing and drops a unit it does not know', () => {
    expect(parseRoomWeather({ sky: 'blizzard', temp: -7, unit: 'K' })).toEqual({
      sky: 'blizzard',
      temp: -7,
      unit: null,
      region: null,
    });
    expect(parseRoomWeather(null)).toBeNull();
  });
});
