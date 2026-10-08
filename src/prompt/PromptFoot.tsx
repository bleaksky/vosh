import { useId } from 'react';
import { previewOptions } from './promptSettings';
import type { PromptShow, PromptShowState } from '../ipc/prompt';
import type { PromptPreviewName } from '../ipc/promptDesign';
import { Button, Toggle } from '../ui';
import { MenuButton } from './MenuButton';
import { ShowButton } from './PromptShow';

// The foot under your design in Customize prompt, on one row 52 tall:
// Draw your prompt and where your prompt shows on the left, the preview
// and Done on the right. The preview shows only while drawing is on.

/** The button's name and the menu's. */
const PREVIEW = 'Preview';

interface PreviewButtonProps {
  value: PromptPreviewName;
  /** Lament joins the previews under the Forsaken Lands rules. */
  forsaken: boolean;
  onChange: (preview: PromptPreviewName) => void;
}

/** The preview: a compact menu button that reads Preview: and the
 *  preview now, and opens a menu of the previews above it, their right
 *  edges together, the current one checked. A screen reader hears
 *  Preview and the preview now. */
export function PreviewButton({ value, forsaken, onChange }: PreviewButtonProps) {
  return (
    <MenuButton
      name={PREVIEW}
      lead={`${PREVIEW}:`}
      choices={previewOptions(forsaken)}
      value={value}
      place="above-end"
      onChange={onChange}
    />
  );
}

interface DesignFootProps {
  draw: boolean;
  onDraw: (draw: boolean) => void;
  /** The place the card's table holds. */
  show: PromptShow;
  /** Whether the profile reads a prompt, or null until it is known. */
  showState: PromptShowState | null;
  onShow: (place: PromptShow) => void;
  preview: PromptPreviewName;
  /** Lament joins the previews under the Forsaken Lands rules. */
  forsaken: boolean;
  onPreview: (preview: PromptPreviewName) => void;
  onDone: () => void;
}

export function DesignFoot({
  draw,
  onDraw,
  show,
  showState,
  onShow,
  preview,
  forsaken,
  onPreview,
  onDone,
}: DesignFootProps) {
  const drawId = useId();
  return (
    <div className="pc-foot">
      <Toggle id={drawId} checked={draw} onChange={onDraw} />
      <label className="pc-switch" htmlFor={drawId}>
        Draw your prompt
      </label>
      <ShowButton value={show} state={showState} onChange={onShow} />
      <div className="pc-foot-end">
        {draw && <PreviewButton value={preview} forsaken={forsaken} onChange={onPreview} />}
        <Button variant="primary" className="pc-done" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  );
}

interface TextFootProps {
  /** Where the text draws, such as Draws in your panel. */
  note: string;
  preview: PromptPreviewName;
  forsaken: boolean;
  onPreview: (preview: PromptPreviewName) => void;
  onDone: () => void;
}

/** The foot under your vitals text: where it draws
 *  on the left, since Style and Show your vitals in say whether and
 *  where, then the preview and Done. */
export function TextFoot({ note, preview, forsaken, onPreview, onDone }: TextFootProps) {
  return (
    <div className="pc-foot">
      <span className="pc-foot-note">{note}</span>
      <div className="pc-foot-end">
        <PreviewButton value={preview} forsaken={forsaken} onChange={onPreview} />
        <Button variant="primary" className="pc-done" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  );
}
