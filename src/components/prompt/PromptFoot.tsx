import { useId } from 'react';
import { previewOptions } from '../../lib/promptSettings';
import type { PromptShowState } from '../../lib/promptShow';
import type { PromptPreviewName, PromptShow } from '../../lib/session';
import { Button, Segmented, Toggle } from '../settings/ui';
import { ShowButton } from './PromptShow';

// The foot under your design in Customize prompt: Draw your prompt and
// where your prompt shows on the left, the preview and Done on the
// right. The preview shows only while drawing is on.

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
    <div className="pc-foot is-design">
      <Toggle id={drawId} checked={draw} onChange={onDraw} />
      <label className="pc-switch" htmlFor={drawId}>
        Draw your prompt
      </label>
      <ShowButton value={show} state={showState} onChange={onShow} />
      <div className="pc-foot-end">
        {draw && (
          <Segmented
            label="Preview"
            options={previewOptions(forsaken)}
            value={preview}
            onChange={onPreview}
          />
        )}
        <Button variant="primary" className="pc-done" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  );
}
