// The times the Scripts page shows, on the 24 hour clock, apart from
// the clockTime of src/prompt/cardRules.ts, which writes 5:04 as
// #prompt says a time.

const two = (n: number) => String(n).padStart(2, '0');

/** `ms` as the local time of day on the 24 hour clock, two digits each,
 *  like `21:14:03`, the time a Lua line shows. */
export function lineTime(ms: number): string {
  const at = new Date(ms);
  return `${saveTime(ms)}:${two(at.getSeconds())}`;
}

/** `ms` as the local hour and minute, like `21:14`, the time the save
 *  bar of a plugin's page shows. */
export function saveTime(ms: number): string {
  const at = new Date(ms);
  return `${two(at.getHours())}:${two(at.getMinutes())}`;
}
