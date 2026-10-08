import type { Dispatch, MouseEvent, MutableRefObject, RefObject, SetStateAction } from 'react';
import { TERMINAL_LINE_HEIGHTS } from '../ipc/uiConfig';
import { PromptDock } from '../prompt/PromptDock';
import type { CellSize } from '../prompt/pinnedDock';
import type { usePromptShow } from '../prompt/showState';
import { FindToolbar } from '../terminal/FindToolbar';
import { Resizable } from '../terminal/Resizable';
import { ScrollDepth } from '../terminal/ScrollDepth';
import { Terminal } from '../terminal/Terminal';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import type { ScrollbackFind } from './useFind';
import type { ScrollbackSplit } from './useScrollbackSplit';
import type { SessionTerminals } from './useSessionTerminals';
import type { useUiConfigFollow } from './useUiConfigFollow';

type TerminalAreaArgs = Pick<
  ScrollbackSplit,
  'splitOpen' | 'historyReady' | 'historyScrollPos' | 'onHistoryLoaded' | 'onHistoryScroll'
> &
  Pick<
    ScrollbackFind,
    'findOpen' | 'findToolbarRef' | 'finds' | 'closeFind' | 'submitFind' | 'onFindResults'
  > &
  Pick<SessionTerminals, 'termRef' | 'onTerminalReady' | 'onScrollbackLoaded'> &
  Pick<
    ReturnType<typeof useUiConfigFollow>,
    | 'renderFamily'
    | 'fontSize'
    | 'terminalLineHeight'
    | 'themeTerminalColors'
    | 'brightBold'
    | 'blinkText'
    | 'scrollbackLines'
  > & {
    terminalAreaRef: RefObject<HTMLDivElement>;
    historyTermRef: MutableRefObject<TerminalHandle | null>;
    handleTerminalMouseUp: (event: MouseEvent<HTMLDivElement>) => void;
    setTerminalMenu: Dispatch<SetStateAction<{ x: number; y: number } | null>>;
    opened: number[];
    selected: number;
    setCellSize: Dispatch<SetStateAction<CellSize | null>>;
    cellSize: CellSize | null;
    promptShow: ReturnType<typeof usePromptShow>;
    promptPinned: boolean;
    promptLifted: boolean;
    dockLent: number;
    dockShows: boolean;
  };

// The main window's terminal slot: the find bars, the scroll depth chip,
// the scrollback split, the live terminal of each opened session and the
// pinned prompt dock. A plain function and no component, so MainWindow
// renders the same element tree and the live Terminal never remounts.
export function terminalArea({
  terminalAreaRef,
  splitOpen,
  handleTerminalMouseUp,
  setTerminalMenu,
  finds,
  opened,
  selected,
  findToolbarRef,
  submitFind,
  closeFind,
  findOpen,
  historyScrollPos,
  historyReady,
  termRef,
  renderFamily,
  fontSize,
  terminalLineHeight,
  themeTerminalColors,
  blinkText,
  scrollbackLines,
  historyTermRef,
  onHistoryLoaded,
  onHistoryScroll,
  onTerminalReady,
  onScrollbackLoaded,
  onFindResults,
  setCellSize,
  promptLifted,
  dockLent,
  dockShows,
  promptPinned,
  promptShow,
  cellSize,
  brightBold,
}: TerminalAreaArgs) {
  return (
    <div
      ref={terminalAreaRef}
      className={`terminal-area${splitOpen ? ' terminal-area-split' : ''}`}
      onMouseUp={handleTerminalMouseUp}
      onContextMenu={(event) => {
        // Replace the webview's default context menu with ours.
        event.preventDefault();
        setTerminalMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      {/* Each opened session with its find bar open keeps it, with its
          query, and the selected session's shows. */}
      {[...finds]
        .filter(([id]) => opened.includes(id))
        .map(([id, results]) => (
          <FindToolbar
            key={id}
            ref={id === selected ? findToolbarRef : undefined}
            hidden={id !== selected}
            results={results}
            onFindNext={(query, opts) => submitFind(query, opts, 'next')}
            onFindPrevious={(query, opts) => submitFind(query, opts, 'previous')}
            onClose={closeFind}
          />
        ))}
      <ScrollDepth findOpen={findOpen} history={historyScrollPos} />
      {/* The containing block for the scrollback split, where the well
          splits wrapper was. It never changes, so opening the split or
          toggling the panel never remounts the live Terminal. */}
      <div className="terminal-well">
        {splitOpen && (
          // History pane is a Resizable so the user can drag the
          // divider between history and live to set the split ratio.
          // anchor=top places the panel at the top with the drag
          // handle on the bottom edge facing the live pane below.
          // Lazy mount. A hidden Terminal cannot be measured by
          // FitAddon (its container is display:none, bounding rect
          // 0x0) so writes wrap at 1-3 columns and the scrollback
          // arrives mangled. The initial scroll runs in
          // onScrollbackLoaded — onReady fires before loadScrollback
          // resolves, and a scrollPages call on an empty terminal
          // is a no-op that the next write would override anyway.
          <Resizable
            storageKey="vosh.layout.splitHistoryHeight"
            defaultSize={240}
            minSize={80}
            maxSize={1200}
            reservePx={120}
            className={`terminal-pane terminal-pane-history${historyReady ? '' : ' terminal-pane-history-priming'}`}
            handleLabel="resize scrollback split"
            snapPx={() => termRef.current?.cellHeight() ?? 0}
          >
            <Terminal
              key={selected}
              session={selected}
              fontFamily={renderFamily}
              fontSize={fontSize}
              lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
              themeTerminalColors={themeTerminalColors}
              blinkText={blinkText}
              scrollback={scrollbackLines}
              quiet
              onReady={(handle) => {
                historyTermRef.current = handle;
              }}
              onScrollbackLoaded={onHistoryLoaded}
              onScrollPosition={onHistoryScroll}
            />
          </Resizable>
        )}
        {/* One live terminal for each session this window opened, keyed
            by session, so a selection only shows one and hides another.
            A session launch restored opens as its first selection
            finishes, and its scrollback loads then. */}
        <div className="terminal-pane terminal-pane-live">
          {opened.map((id) => (
            <Terminal
              key={id}
              session={id}
              shown={id === selected}
              fontFamily={renderFamily}
              fontSize={fontSize}
              lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
              themeTerminalColors={themeTerminalColors}
              blinkText={blinkText}
              scrollback={scrollbackLines}
              onReady={onTerminalReady(id)}
              onScrollbackLoaded={onScrollbackLoaded(id)}
              onResultsChanged={(event) => onFindResults(id, event)}
              onCellSize={setCellSize}
              lifted={promptLifted}
              lentRows={dockLent}
              anchorBottom={dockShows}
            />
          ))}
        </div>
      </div>
      {/* Your prompt pinned above the command line. It takes one row from
          the terminal only while your prompt shows pinned and a prompt
          is pinned, and borrows the rows past its first from the bottom
          of the live pane. */}
      {promptPinned && promptShow && cellSize && (
        <PromptDock
          state={promptShow}
          cell={cellSize}
          fontSize={fontSize}
          themeTerminalColors={themeTerminalColors}
          brightBold={brightBold}
          renderer={nativeSurfaceEnabled() ? 'native' : 'xterm'}
          blinkText={blinkText}
        />
      )}
    </div>
  );
}
