/** `ms` as the local time of day on the 24 hour clock, two digits each,
 *  like `21:14:03`, the time a Lua line shows. */
export function clockTime(ms: number): string {
  const at = new Date(ms);
  const two = (n: number) => String(n).padStart(2, '0');
  return `${two(at.getHours())}:${two(at.getMinutes())}:${two(at.getSeconds())}`;
}
