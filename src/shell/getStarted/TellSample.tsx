import { useMemo, type CSSProperties } from 'react';
import { chatChannelSlot, chatInks } from '../../panel/chat/chatColors';
import { useChatColors } from '../../stores/config/chatColorsStore';
import { usePlayPalette } from '../../theme/fitGameColors';
import { themeTokens } from '../../theme/themes';
import { useActiveTheme } from '../../theme/useActiveTheme';

/** A tell you sent as the Chat pane shows it, on the panel in the tell
 *  color, with bars for the words, so the sample never puts words in a
 *  player's mouth. */
export function TellSample() {
  const theme = useActiveTheme();
  const palette = usePlayPalette();
  const colors = useChatColors();
  const ink = useMemo(
    () => chatInks(palette, themeTokens(theme))[chatChannelSlot('tell', colors)],
    [palette, theme, colors],
  );
  const bar = (width: string) => (
    <span className="st-auto-sample-bar" style={{ width }} role="img" aria-label="Words" />
  );
  return (
    <div
      className="st-auto-sample gs-sample is-panel"
      style={{ '--sample-fg': ink.color } as CSSProperties}
    >
      <span className={ink.fadeTag ? 'pane-chat-tag' : 'pane-chat-tag is-solid'}>[tell]</span>
      to <span className="pane-chat-speaker">Tolliver</span>: {bar('15ch')} {bar('8ch')}
    </div>
  );
}
