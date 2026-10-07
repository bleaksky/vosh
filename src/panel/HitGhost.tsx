// The part of a bar Show each hit leaves pale, from the fill at `fill`
// to `ghost`, both in percent. Once `draining` it shrinks to the fill,
// over the time panel.css gives it (vitalsHit.ts).

export function HitGhost({
  className,
  fill,
  ghost,
  draining,
}: {
  className: string;
  fill: number;
  ghost: number;
  draining: boolean;
}) {
  return (
    <span
      className={`${className} vitals-ghost${draining ? ' is-draining' : ''}`}
      style={{ left: `${fill}%`, width: `${draining ? 0 : Math.max(0, ghost - fill)}%` }}
    />
  );
}
