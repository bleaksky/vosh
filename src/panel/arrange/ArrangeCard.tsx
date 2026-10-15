import { useState, type KeyboardEvent } from 'react';
import { flushSync } from 'react-dom';
import { paneHistoryGet, type EarlierLayout } from '../../ipc/panes';
import { profileDisplayName } from '../../lib/characterProfiles';
import { useEscape } from '../../lib/escapeStack';
import { Button } from '../../ui';
import { GripIcon } from '../../ui/icons';
import type { MenuPlacement } from '../../ui/MenuSurface';
import { menuBelow } from '../../ui/menuPlacement';
import { EarlierLayoutsMenu } from '../EarlierLayoutsMenu';
import { bothTrees } from '../paneLayout';
import { luaPanesToAdd, offeredLuaPanes, paneTypesToAdd } from '../paneTypes';
import { useLuaPanes } from '../../stores/session/luaPanesStore';
import { usePluginRows } from '../../stores/session/pluginRowsStore';
import {
  arrangeView,
  putBack,
  shelfItems,
  undoArranging,
  useArranging,
  type ShelfItem,
} from './arrange';
import { arrangeDefault, arrangeEarlier, cancelArranging, finishArranging } from './arrangeActions';
import { tileDrag } from './arrangeDrag';
import { ARRANGE_KEYS_ID, focusTile } from './tileFocus';

// The card that takes the terminal's column while you arrange. It says
// whose panes these are and how to move them, then holds the shelf: the
// panes you put away and every pane you could add, to drag out or put
// back. Under it, Start from the default and Earlier layouts start the
// draft again, Undo takes back a step, Cancel and Esc drop the draft,
// and Done saves it.

interface EarlierOpen {
  earlier: EarlierLayout[];
  at: MenuPlacement;
  anchor: HTMLButtonElement;
}

export function ArrangeCard({ sidebarFits }: { sidebarFits: boolean }) {
  const a = useArranging();
  const [earlier, setEarlier] = useState<EarlierOpen | null>(null);
  // The shelf offers the Lua panes the running plugins draw now.
  const luaPanes = useLuaPanes();
  const pluginRows = usePluginRows();
  useEscape(a !== null && earlier === null && !a.drag, cancelArranging);
  if (!a) return null;
  const view = arrangeView(a);
  const both = bothTrees(view.layout);
  const offers = {
    builtIns: paneTypesToAdd(both),
    lua: luaPanesToAdd(both, offeredLuaPanes(luaPanes, pluginRows)),
  };
  const items = shelfItems(view.layout, view.putAway, offers);
  const owner = profileDisplayName(a.profile);

  const openEarlier = (button: HTMLButtonElement) => {
    if (earlier) {
      setEarlier(null);
      return;
    }
    const at = menuBelow(button.getBoundingClientRect());
    paneHistoryGet(a.profile)
      .catch(() => [])
      .then((list) => setEarlier({ earlier: list, at, anchor: button }));
  };

  return (
    <div className="arrange-card" role="region" aria-label="Arrange panes">
      <h2 className="arrange-title">Arranging the panes for {owner}</h2>
      <p className="arrange-line">
        Drag a pane to move it. Drop it on the left or right edge of another pane to set the two
        side by side, or here to put it away.
      </p>
      {!sidebarFits && (
        <p className="arrange-line">Make the window wider to keep panes beside your sessions.</p>
      )}
      <div className="arrange-shelf-head">
        Not showing <span>{items.length > 0 ? items.length : ''}</span>
      </div>
      <div
        className={`arrange-shelf${a.drag?.target?.kind === 'shelf' ? ' is-over' : ''}`}
        data-arrange-shelf=""
      >
        {items.length === 0 ? (
          <p className="arrange-shelf-empty">Every pane is showing.</p>
        ) : (
          items.map((item) =>
            item.leaf.id === view.slot ? (
              <div key={item.leaf.id} className="arrange-shelf-tile is-slot" aria-hidden="true">
                {item.label}
              </div>
            ) : (
              <ShelfTile key={item.leaf.id} item={item} />
            ),
          )
        )}
      </div>
      <div className="arrange-actions">
        <button type="button" className="arrange-plain" onClick={arrangeDefault}>
          Start from the default
        </button>
        <button
          type="button"
          className="arrange-plain"
          aria-haspopup="menu"
          aria-expanded={earlier !== null}
          onClick={(e) => openEarlier(e.currentTarget)}
        >
          Earlier layouts
        </button>
        <span className="arrange-actions-gap" />
        <Button onClick={undoArranging} disabled={a.undo.length === 0}>
          Undo
        </Button>
        <Button onClick={cancelArranging}>Cancel</Button>
        <Button variant="primary" onClick={() => void finishArranging()}>
          Done
        </Button>
      </div>
      <p id={ARRANGE_KEYS_ID} className="arrange-keys">
        Up and Down move the pane you picked past its neighbor, and the arrow toward the other side
        moves it there. Delete puts it away. Esc cancels.
      </p>
      {earlier && (
        <EarlierLayoutsMenu
          earlier={earlier.earlier}
          at={earlier.at}
          anchor={earlier.anchor}
          onClose={(reason) => {
            setEarlier(null);
            if (reason === 'escape') earlier.anchor.focus();
          }}
          onPick={(until) => {
            const picked = earlier.earlier.find((e) => e.until === until);
            earlier.anchor.focus();
            setEarlier(null);
            if (picked) arrangeEarlier(picked.panes);
          }}
        />
      )}
    </div>
  );
}

function ShelfTile({ item }: { item: ShelfItem }) {
  const back = () => {
    flushSync(() => putBack(item));
    focusTile(item.leaf.id);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget || (e.key !== 'Enter' && e.key !== ' ')) return;
    e.preventDefault();
    back();
  };
  return (
    <div
      className="arrange-shelf-tile"
      role="button"
      tabIndex={0}
      data-shelf-tile={item.leaf.id}
      data-shelf-pane={item.leaf.pane}
      aria-label={`${item.label}, drag to show it, or press Enter to show it`}
      onPointerDown={tileDrag(item.leaf)}
      onKeyDown={onKeyDown}
    >
      <GripIcon className="arrange-grip" />
      <span className="arrange-tile-name">{item.label}</span>
      <button type="button" className="arrange-shelf-show" onClick={back}>
        Show
      </button>
    </div>
  );
}
