import { memo } from 'react';
import { glyphGrid, type MapTilesPayload } from './mapTiles';
import { cellClass } from './mapPaint';

// HTML glyph overlay.
//
// glyphGrid() in panel/map/mapTiles lays out the rooms and the connectors
// between them. The player cell anchors to the parent's geometric
// center via a CSS calc() translate, so the map scrolls around the
// player as they walk. Each cell measures 1ch × 1em (line-height: 1)
// so the math is a clean character count.
//
// Wrapped in React.memo with a content-based equality. Props compare
// via the payload's cached JSON string (plus zoom), not the payload
// object reference, so a fresh-but-identical payload object cannot
// defeat the memo. A Char.Vitals / Room.Info / Room.Chars burst
// during a movement (which re-renders MapView when its snapshot
// or themeVersion shifts) does not re-run the 60×60 grid build +
// ~3500-span DOM diff. Only real payload content changes or the zoom
// value trigger a rebuild.
export const GlyphsOverlay = memo(
  function GlyphsOverlay({
    payload,
    zoom,
  }: {
    payload: MapTilesPayload;
    /// Serialized form of `payload`, cached by the parent. The memo
    /// equality below compares this string instead of the payload
    /// reference.
    payloadJson: string;
    zoom: number;
  }) {
    const grid = glyphGrid(payload);
    if (!grid) {
      return <div className="map-glyph-empty">no glyph data in payload</div>;
    }
    const { cells: out, centerR, centerC } = grid;

    // Font size scales with zoom; base 14 keeps glyph cells legible at
    // 1.0× and matches the terminal's default size.
    const fontSize = Math.round(14 * zoom);

    // Asymmetric cell sizing crunches the map toward squares-mode
    // density while keeping connection chars visible. Room cells stay
    // 1em × 1em so each sector glyph still has a clean box. Bridge
    // cells (the in-between row/column the doubled grid creates for
    // ─ │ connectors) shrink to BRIDGE_EM, so room-to-room pitch is
    // 1 + BRIDGE_EM em instead of 2em. CSS reads this constant via
    // a --vosh-glyph-bridge custom property so the same value drives
    // both cell widths and the player-center translate.
    const BRIDGE_EM = 0.35;
    const ROOM_EM = 1.0;
    const stepEm = ROOM_EM + BRIDGE_EM;
    const playerColOffset = centerC * stepEm + ROOM_EM / 2;
    const playerRowOffset = centerR * stepEm + ROOM_EM / 2;

    return (
      <div
        className="map-glyph-grid"
        style={{
          fontSize: `${fontSize}px`,
          transform: `translate(calc(-1em * ${playerColOffset}), calc(-1em * ${playerRowOffset}))`,
        }}
      >
        {out.map((row, r) => (
          <div
            key={r}
            className={`map-glyph-row ${r % 2 === 0 ? 'map-glyph-row-room' : 'map-glyph-row-bridge'}`}
          >
            {row.map((cell, c) => (
              <span key={c} className={cellClass(cell, r, c)} style={{ color: cell.color }}>
                {cell.glyph}
              </span>
            ))}
          </div>
        ))}
      </div>
    );
  },
  (prev, next) => prev.payloadJson === next.payloadJson && prev.zoom === next.zoom,
);
