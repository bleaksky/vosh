import { useEffect, useId, useMemo, useState } from 'react';
import type { BandEnv } from '../../terminal/bandCells';
import { previewHeight, previewRows } from '../../prompt/promptSettings';
import type { PromptFieldState } from '../../ipc/prompt';
import { promptDescribe, promptRender, type PromptPreviewName } from '../../ipc/promptDesign';
import { shownColumns, type Cell } from '../../terminal/sgrCells';
import { warnBoxes, warnedPieces } from '../../prompt/promptWarn';
import { CARD_ROW_PX } from '../../lib/useCellWidth';
import { CellLine } from '../../prompt/PromptCells';
import { Segmented } from '../../ui';
import { useShown } from '../shownProfile';

// ---------------------------------------------------------------------
// The preview
// ---------------------------------------------------------------------

/** The text inset of the preview output. */
const OUT_X = 10;
const OUT_W = 600;
/** The band reaches 4 px past the text each side and 2 px past each
 *  row, so the text never touches its edge. */
const BAND_X = 4;
const BAND_Y = 2;

interface PreviewBlockProps {
  template: string;
  /** What each value reads now, to ring a part no value fills. */
  catalog: readonly PromptFieldState[];
  /** Draw live values, or samples while you are offline. */
  live: boolean;
  preview: PromptPreviewName;
  options: { value: PromptPreviewName; label: string }[];
  onPreview: (preview: PromptPreviewName) => void;
  /** Draw the design on the band, while your prompt shows lifted or
   *  pinned. */
  band: boolean;
  env: BandEnv;
  cellW: number;
  meta: string;
  /** Changes whenever the values may have, to draw again. */
  tick: unknown;
}

/** The preview: the Segmented, then your design drawn by prompt_render
 *  on the terminal ground, 600 wide and 28 tall plus 17.5 for each line
 *  past the first, then where to change it. It never changes the prompt
 *  on screen. */
export function PreviewBlock({
  template,
  catalog,
  live,
  preview,
  options,
  onPreview,
  band,
  env,
  cellW,
  meta,
  tick,
}: PreviewBlockProps) {
  const [drawn, setDrawn] = useState<{ ansi: string; rings: Box[] } | null>(null);
  // The session the Settings header names, whose values draw the design.
  const session = useShown().session ?? undefined;
  useEffect(() => {
    let alive = true;
    const shown = preview === 'now' ? null : preview;
    // A part no value fills draws its label in the ring, as the card
    // draws it, so you see which part stays blank. Only live values
    // leave a part blank.
    const ringed = live
      ? promptDescribe(template, shown, null, session)
          .then((d) => warnedPieces(d.pieces, d.tokens, catalog))
          .catch(() => new Set<number>())
      : Promise.resolve(new Set<number>());
    void ringed
      .then((warn) =>
        promptRender(
          {
            template,
            values: live ? 'live' : 'sample',
            preview: shown,
            placeholders: warn.size > 0,
          },
          session,
        ).then((rendered) => ({
          ansi: rendered.ansi,
          rings: warnBoxes(rendered.spans, warn, {
            x: OUT_X,
            y: (28 - CARD_ROW_PX) / 2,
            cellW,
            rowH: CARD_ROW_PX,
          }),
        })),
      )
      .then((next) => {
        if (alive) setDrawn(next);
      })
      .catch(() => {
        if (alive) setDrawn(null);
      });
    return () => {
      alive = false;
    };
  }, [template, catalog, live, preview, cellW, tick, session]);
  const rows = useMemo(() => previewRows(drawn?.ansi ?? null), [drawn]);
  return (
    <PreviewView
      rows={rows}
      rings={drawn?.rings ?? []}
      preview={preview}
      options={options}
      onPreview={onPreview}
      band={band}
      env={env}
      cellW={cellW}
      meta={meta}
    />
  );
}

/** A ring's place in the preview output, in px. */
interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface PreviewViewProps {
  rows: Cell[][];
  /** The rings round parts no value fills. */
  rings?: readonly Box[];
  preview: PromptPreviewName;
  options: { value: PromptPreviewName; label: string }[];
  onPreview: (preview: PromptPreviewName) => void;
  band: boolean;
  env: BandEnv;
  cellW: number;
  meta: string;
}

/** The preview as drawn. Exported for its test. */
export function PreviewView({
  rows,
  rings = [],
  preview,
  options,
  onPreview,
  band,
  env,
  cellW,
  meta,
}: PreviewViewProps) {
  const labelId = useId();
  const height = previewHeight(rows.length);
  const top = (28 - CARD_ROW_PX) / 2;
  const limit = Math.floor((OUT_W - 2 * OUT_X) / cellW);
  const widest = Math.min(limit, Math.max(0, ...rows.map(shownColumns)));
  return (
    <div className="st-block st-prompt-preview-block" data-st-anchor="prompt-preview">
      <div className="st-prompt-preview-head">
        <span id={labelId} className="st-row-label">
          Preview
        </span>
        <Segmented
          label="Preview"
          options={options}
          value={preview}
          onChange={onPreview}
          className="st-prompt-preview-seg"
        />
      </div>
      <output
        className="st-prompt-preview"
        aria-label="Your prompt as Vosh draws it"
        style={{ height, color: env.fg }}
      >
        {band && widest > 0 && (
          <span
            className="prompt-band"
            aria-hidden="true"
            style={{
              left: OUT_X - BAND_X,
              top: top - BAND_Y,
              width: widest * cellW + 2 * BAND_X,
              height: rows.length * CARD_ROW_PX + 2 * BAND_Y,
            }}
          />
        )}
        {rings.map((ring, i) => (
          <span key={i} className="st-prompt-preview-warn" aria-hidden="true" style={ring} />
        ))}
        {rows.map((cells, i) => (
          <CellLine
            key={i}
            cells={cells}
            env={env}
            cellW={cellW}
            limit={limit}
            className="st-prompt-preview-row"
            style={{ top: top + i * CARD_ROW_PX }}
          />
        ))}
      </output>
      <p className="st-prompt-meta">{meta}</p>
    </div>
  );
}
