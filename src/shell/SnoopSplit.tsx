import { snoopClose, snoopStop } from '../ipc/snoop';
import { useSnoopShare } from '../stores/config/snoopShareStore';
import { selectSnoop, useSnoops } from '../stores/session/snoopStore';
import { pushToast } from '../stores/toasts';
import { SnoopTerminal } from '../terminal/SnoopTerminal';
import { EyeIcon } from './icons';
import { useMinuteClock } from './sessionLine';
import { endedLine, tabTitle } from './snoopLine';

// The snoop split at the top of the terminal column, boards 01 and 03 of
// the Snoop review (SN1, SN2, SN3 and SN5). A strip with the eye and a
// tab for each player the selected session snoops, each with the
// sessions sidebar's mark, a dot while it runs and a ring once it ended,
// and an accent dot on a tab behind with lines you have not read. Stop
// sends `snoop stop` with the player in front, and an ended tab says when
// it ended and offers Close. Under the strip, a terminal for each tab,
// the one in front shown.
//
// It opens at the profile's saved share of the column once a snoop
// starts, and shows nothing with no tab or while the session's snoops
// sit in their window. Nothing here takes the caret: a start leaves it
// on the command line, and a press on the strip hands it back there.

interface Props {
  session: number;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  themeTerminalColors: boolean;
  /** Put the caret back on the command line. */
  onCaret: () => void;
}

export function SnoopSplit({
  session,
  fontFamily,
  fontSize,
  lineHeight,
  themeTerminalColors,
  onCaret,
}: Props) {
  const { tabs, windowed, selected, unread } = useSnoops();
  const share = useSnoopShare();
  const now = useMinuteClock();
  if (tabs.length === 0 || windowed) return null;
  const front = tabs.find((tab) => tab.name === selected) ?? null;

  const failed = (e: unknown) => pushToast({ kind: 'error', message: String(e) });
  const act = (run: () => Promise<void>) => {
    run().catch(failed);
    onCaret();
  };

  return (
    <section className="snoop" style={{ height: `${share * 100}%` }} aria-label="Snoop">
      <div className="snoop-strip">
        <span className="snoop-eye" role="img" aria-label="Snoop">
          <EyeIcon />
        </span>
        <div className="snoop-tabs" role="tablist" aria-label="Snooped players">
          {tabs.map((tab) => (
            <button
              key={tab.name}
              type="button"
              role="tab"
              className={
                'snoop-tab' +
                (unread.has(tab.name) ? ' is-unread' : '') +
                (tab.live ? '' : ' is-ended')
              }
              aria-selected={tab.name === selected}
              title={tabTitle(tab, now)}
              onClick={() => {
                selectSnoop(tab.name);
                onCaret();
              }}
            >
              <span className="snoop-mark">
                <span className="snoop-dot" />
              </span>
              <span className="snoop-name">{tab.name}</span>
            </button>
          ))}
        </div>
        {front && (
          <div className="snoop-end">
            {!front.live && <span className="snoop-meta">{endedLine(front, now)}</span>}
            <button
              type="button"
              className="snoop-btn"
              onClick={() =>
                act(() =>
                  front.live ? snoopStop(session, front.name) : snoopClose(session, front.name),
                )
              }
            >
              {front.live ? 'Stop' : 'Close'}
            </button>
          </div>
        )}
      </div>
      <div className="snoop-body">
        {tabs.map((tab) => (
          <SnoopTerminal
            key={`${session}:${tab.name}`}
            session={session}
            name={tab.name}
            shown={tab.name === selected}
            fontFamily={fontFamily}
            fontSize={fontSize}
            lineHeight={lineHeight}
            themeTerminalColors={themeTerminalColors}
          />
        ))}
      </div>
    </section>
  );
}
