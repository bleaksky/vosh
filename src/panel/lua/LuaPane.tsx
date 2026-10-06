import { useEffect, useMemo, type CSSProperties, type ReactNode } from 'react';
import { colorize } from '../../automation/colorTokens';
import type { LuaBlock, LuaPane as LuaPaneData } from '../../ipc/panes';
import type { PluginRow } from '../../ipc/scripts';
import { useLuaPane } from '../../stores/session/luaPanesStore';
import { usePluginRows } from '../../stores/session/pluginRowsStore';
import { ANSI_SLOTS, type AnsiSlot } from '../../theme/baseAnsi';
import { indexedRgb, toHex } from '../../theme/color';
import { usePlayPalette } from '../../theme/fitGameColors';
import { themeTokens, type XtermPalette } from '../../theme/themes';
import { useActiveTheme } from '../../theme/useActiveTheme';
import { parseSgrCells, type CellAttrs, type CellColor } from '../../terminal/sgrCells';
import { Button } from '../../ui/Button';
import { chatInks, type ChatGround, type ChatInk } from '../chat/chatColors';
import { closeHere, updateLeafProps, usePaneLeaf } from '../paneActions';
import { PaneHeader, PaneMeta } from '../PaneHeader';

// A pane a plugin draws with mud.pane (Scripts and Panels board 10),
// from the session in front. Its header names it by the title the plugin
// draws now, else the last title it showed, else its id, with the
// plugin's meta beside it. Rows and gauges line up like the Group pane.
// A line takes Vosh color codes like {red}, as Replace with does, drawn
// in the game face with each color lifted to 3:1 on the panel as chat
// does. A rule is a hairline. Every word is a text node, so a plugin
// cannot put markup on the page.
//
// While the plugin draws nothing, the pane says why: the plugin is off,
// Vosh stopped it, or you removed it, which offers to close the pane. A
// plugin that is on but has not drawn the pane yet leaves the body
// empty, since no board draws that case.

export function LuaPane() {
  const leaf = usePaneLeaf();
  const plugin = leaf?.props.plugin ?? '';
  const id = leaf?.props.id ?? '';
  const pane = useLuaPane(plugin, id);
  const plugins = usePluginRows();
  const theme = useActiveTheme();
  const palette = usePlayPalette();
  const ground = useMemo(() => themeTokens(theme), [theme]);
  const live = pane?.title.trim() ?? '';
  const leafId = leaf?.id;
  const saved = leaf?.props.title;

  // The leaf keeps the last title, so the pane names itself while its
  // plugin is not running.
  useEffect(() => {
    if (leafId !== undefined && live.length > 0 && live !== saved) {
      updateLeafProps(leafId, { title: live });
    }
  }, [leafId, live, saved]);

  if (!leaf) return null;
  return (
    <LuaPaneView
      plugin={plugin}
      title={live || saved?.trim() || id}
      pane={pane}
      row={plugins === null ? undefined : (plugins.find((p) => p.name === plugin) ?? null)}
      palette={palette}
      ground={ground}
      onClose={() => closeHere(leaf.id)}
    />
  );
}

/** The pane drawn from plain values, so each state renders in a test.
 *  `row` is the plugin's row, null when you removed the plugin and
 *  undefined before the list comes. */
export function LuaPaneView({
  plugin,
  title,
  pane,
  row,
  palette,
  ground,
  onClose,
}: {
  plugin: string;
  title: string;
  pane: LuaPaneData | undefined;
  row: PluginRow | null | undefined;
  palette: XtermPalette;
  ground: ChatGround;
  onClose: () => void;
}) {
  const inks = useMemo(() => chatInks(palette, ground), [palette, ground]);
  const meta = pane?.meta.trim() ?? '';

  let body: ReactNode = null;
  if (pane) {
    body = (
      <ul className="pane-rows">
        {pane.blocks.map((block, i) => (
          <Block key={i} block={block} palette={palette} inks={inks} />
        ))}
      </ul>
    );
  } else if (row === null) {
    body = (
      <>
        <p className="pane-empty">You removed {plugin}, so nothing fills this pane.</p>
        <div className="pane-lua-close">
          <Button onClick={onClose}>Close pane</Button>
        </div>
      </>
    );
  } else if (row?.stopped) {
    body = (
      <p className="pane-empty">
        Vosh stopped {plugin}. Save it under Scripts in Settings or restart Vosh to fill this pane.
      </p>
    );
  } else if (row && !row.on) {
    body = (
      <p className="pane-empty">
        This pane fills when {plugin} is on. Turn it on under Scripts in Settings.
      </p>
    );
  }

  return (
    <>
      <PaneHeader title={title} meta={meta ? <PaneMeta>{meta}</PaneMeta> : null} />
      <div className="pane-body">{body}</div>
    </>
  );
}

function Block({
  block,
  palette,
  inks,
}: {
  block: LuaBlock;
  palette: XtermPalette;
  inks: Record<AnsiSlot, ChatInk>;
}) {
  switch (block.kind) {
    case 'row':
      return (
        <li className="pane-row">
          <span className="pane-row-name">{block.label}</span>
          <span className="pane-row-value">{block.value}</span>
        </li>
      );
    case 'gauge': {
      const pct = gaugePct(block.value, block.max);
      return (
        <li className="pane-row">
          <span className="pane-row-name">{block.label}</span>
          <span className="pane-member-meter" aria-hidden="true">
            <span className="pane-member-fill" style={{ width: `${pct}%` }} />
          </span>
          <span className="pane-member-pct">{`${pct}%`}</span>
        </li>
      );
    }
    case 'line':
      return (
        <li className="pane-lua-line">
          <LineRuns text={block.text} palette={palette} inks={inks} />
        </li>
      );
    case 'rule':
      return <li className="pane-lua-rule" role="separator" />;
  }
}

/** The percent a gauge shows, whole and between 0 and 100. */
function gaugePct(value: number, max: number): number {
  if (!Number.isFinite(value) || !Number.isFinite(max) || max <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((value / max) * 100)));
}

/** A line's text in runs that look alike, each a span with its look. */
function LineRuns({
  text,
  palette,
  inks,
}: {
  text: string;
  palette: XtermPalette;
  inks: Record<AnsiSlot, ChatInk>;
}) {
  const runs: { text: string; style: CSSProperties }[] = [];
  let last: CellAttrs | null = null;
  parseSgrCells(colorize(text)).forEach((cells, i) => {
    if (i > 0) {
      runs.push({ text: '\n', style: {} });
      last = null;
    }
    for (const cell of cells) {
      // The second column of a wide character holds nothing.
      if (cell.ch === '') continue;
      if (last === cell.attrs && runs.length > 0) {
        runs[runs.length - 1].text += cell.ch;
      } else {
        runs.push({ text: cell.ch, style: look(cell.attrs, palette, inks) });
        last = cell.attrs;
      }
    }
  });
  return (
    <>
      {runs.map((run, i) => (
        <span key={i} style={run.style}>
          {run.text}
        </span>
      ))}
    </>
  );
}

/** How a run draws: its color, the 16 ANSI colors lifted to 3:1 on the
 *  panel as chat lifts them and any other as given, with bold, italic,
 *  underline and strikethrough. */
function look(
  attrs: CellAttrs,
  palette: XtermPalette,
  inks: Record<AnsiSlot, ChatInk>,
): CSSProperties {
  const lines = [attrs.underline > 0 && 'underline', attrs.strike && 'line-through'].filter(
    Boolean,
  );
  return {
    color: attrs.fg ? inkOf(attrs.fg, palette, inks) : undefined,
    fontWeight: attrs.bold ? 'bold' : undefined,
    fontStyle: attrs.italic ? 'italic' : undefined,
    textDecorationLine: lines.length > 0 ? lines.join(' ') : undefined,
  };
}

function inkOf(color: CellColor, palette: XtermPalette, inks: Record<AnsiSlot, ChatInk>): string {
  if (color.kind === 'rgb') return toHex(color);
  if (color.n < ANSI_SLOTS.length) return inks[ANSI_SLOTS[color.n]].color;
  return toHex(
    indexedRgb(
      color.n,
      ANSI_SLOTS.map((slot) => palette[slot]),
    ),
  );
}
