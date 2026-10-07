// The part of a bar Show each hit leaves pale, from the fill at `fill`
// to `ghost`, both in percent. It drains toward the fill as panel.css
// says once `draining` (vitalsHit.ts).

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
      style={{ left: `${fill}%`, width: `${Math.max(0, ghost - fill)}%` }}
    />
  );
}
