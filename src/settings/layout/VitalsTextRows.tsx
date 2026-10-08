import { useEffect, useState, type CSSProperties } from 'react';
import { openVitalsTextCard, promptStateGet } from '../../ipc/prompt';
import {
  promptRenderMany,
  type PromptPreviewName,
  type PromptRendered,
} from '../../ipc/promptDesign';
import type { UiConfig } from '../../ipc/uiConfig';
import { drawnVitalsText } from '../../ipc/vitals';
import { isMacPlatform } from '../../lib/shortcuts';
import { panelWidthFloor } from '../../panel/paneLayout';
import { panelWidthOf, usePanelLayout } from '../../panel/panelLayoutStore';
import { panelFontFamily } from '../../panel/panelFont';
import { resolvePanelSize } from '../../panel/panelSize';
import { PaneTextSizeContext } from '../../panel/paneTextSize';
import { VitalsTextBlock } from '../../panel/VitalsText';
import { textRows, type TextLine } from '../../panel/vitalsTextFit';
import { previewOptions, shownPreview } from '../../prompt/promptSettings';
import { Button, Row, Segmented } from '../../ui';
import { useShown } from '../shownProfile';
import { useGalleryVitals, usePanelText } from './usePanelVitals';

// Customize vitals under Text: your text decides which vitals show,
// their order, their colors and how values read, so the section holds
// Your vitals text and the preview of Settings, Input, Prompt, with
// Now, Low health, Fight and Lament, drawn on the panel ground at your
// panel's width as the footer draws it. The preview never changes the
// vitals on screen.

/** Whether the Forsaken Lands rules hold for `session`, which brings
 *  the Lament preview. */
function useForsaken(session: number | undefined): boolean {
  const [forsaken, setForsaken] = useState(false);
  useEffect(() => {
    let open = true;
    promptStateGet(session)
      .then((state) => open && setForsaken(state.forsaken))
      .catch(() => open && setForsaken(false));
    return () => {
      open = false;
    };
  }, [session]);
  return forsaken;
}

/** `template` rendered `cols` cells wide for `preview`, or null until it
 *  renders. */
function usePreviewText(
  template: string,
  live: boolean,
  preview: PromptPreviewName,
  cols: number,
  session: number | undefined,
): PromptRendered | null {
  const [rendered, setRendered] = useState<PromptRendered | null>(null);
  useEffect(() => {
    let open = true;
    promptRenderMany(
      [
        {
          template,
          values: live ? 'live' : 'sample',
          preview: preview === 'now' ? null : preview,
          cols,
        },
      ],
      session,
    )
      .then(([text]) => open && setRendered(text ?? null))
      .catch(() => open && setRendered(null));
    return () => {
      open = false;
    };
  }, [template, live, preview, cols, session]);
  return rendered;
}

export function VitalsTextRows({ config }: { config: UiConfig }) {
  const layout = usePanelLayout();
  const panel = Math.max(panelWidthFloor(isMacPlatform()), panelWidthOf(layout));
  const size = resolvePanelSize(config.panel_font_size, config.font_size);
  const family = panelFontFamily(config.panel_font);
  const { cols, env } = usePanelText(config, panel);
  const { live } = useGalleryVitals();
  const session = useShown().session ?? undefined;
  const forsaken = useForsaken(session);
  const [preview, setPreview] = useState<PromptPreviewName>('now');
  const shown = shownPreview(preview, forsaken);
  const rendered = usePreviewText(drawnVitalsText(config), live, shown, cols, session);
  const lines: TextLine[] = rendered
    ? textRows(rendered, []).map((row) => ({ left: row.cells, right: null }))
    : [];
  const host = {
    '--panel-text-px': String(size),
    ...(family === null
      ? {}
      : { fontFamily: family, '--font-panel': family, '--font-panel-game': family }),
  } as CSSProperties;
  return (
    <>
      <Row
        label="Your vitals text"
        description="Write your vitals with the codes your prompt uses. Your text decides which vitals show, their order, their colors and how values read."
        anchor="vitals-text"
      >
        {/* The card for your text opens over the main window's terminal. */}
        <Button onClick={() => void openVitalsTextCard().catch(() => undefined)}>Edit…</Button>
      </Row>
      <div className="st-block st-vitals-preview" data-st-anchor="vitals-preview">
        <Segmented
          label="Preview"
          options={previewOptions(forsaken)}
          value={shown}
          onChange={setPreview}
        />
        <div className="st-vitals-preview-ground" style={{ width: panel }}>
          <div className="panel-host" style={host}>
            <PaneTextSizeContext.Provider value={size}>
              <VitalsTextBlock lines={lines} env={env} />
            </PaneTextSizeContext.Provider>
          </div>
        </div>
        <p className="st-meta st-vitals-preview-meta">
          {live
            ? `At your panel's width, ${panel} pt. It never changes the vitals on screen.`
            : `At your panel's width, ${panel} pt, with sample values until you connect. It never changes the vitals on screen.`}
        </p>
      </div>
    </>
  );
}
