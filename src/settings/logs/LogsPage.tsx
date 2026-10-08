import { useEffect, useState } from 'react';
import { listLogSessions, logsKeepGet, logsKeepSet } from '../../ipc/logs';
import { DEFAULT_SCROLLBACK_LINES } from '../../ipc/uiConfig';
import { settingsSubpage } from '../../lib/settingsNav';
import { isMacPlatform } from '../../lib/shortcuts';
import { useSessionTarget } from '../../stores/session/useConnection';
import { Button, Row, Section, Select, Toggle } from '../../ui';
import { isLocalHost, KEEP_LOGS, SCROLLBACK_SIZES, savedLogsText } from '../general/logView';
import { ScenePage } from '../general/ScenePage';
import { SessionLogs } from '../general/SessionLogs';
import type { SettingsPageProps } from '../pageTypes';
import { useSettingsAutoSave } from '../useSettingsAutoSave';

// Logs: Session logs and Scrollback, the two things Vosh keeps of what
// you saw, on disk and in the terminal. The tab opens on its settings. Search logs… opens
// the log view inside Logs at logs:search (SessionLogs.tsx), and Save a
// scene…, here or in the log view, opens the scene page at logs:scene
// (ScenePage.tsx). The bare link `logs`, which palette Recent and older
// builds send, opens the search.

export function LogsPage(props: SettingsPageProps) {
  // The log Save a scene… in the log view picked, with the navigation
  // that opens the scene page on it. A scene opened any other way, from
  // this page, the terminal's menu or search, opens on the selected
  // session's newest log. Each navigation opens the page afresh.
  const [sceneFrom, setSceneFrom] = useState<{ log: number; seq: number } | null>(null);
  const { update } = useSettingsAutoSave(props.setConfig, props.onError);
  const { config, onError, navigate } = props;
  if (props.target.section === 'scene') {
    const log = sceneFrom?.seq === props.navSeq ? sceneFrom.log : null;
    return <ScenePage key={props.navSeq} {...props} log={log} />;
  }
  if (settingsSubpage(props.target) !== null) {
    return (
      <SessionLogs
        {...props}
        onSaveScene={(log) => {
          setSceneFrom({ log, seq: props.navSeq + 1 });
          navigate({ group: 'logs', section: 'scene' });
        }}
      />
    );
  }
  return (
    <>
      <SessionLogsSection
        logSessions={config?.log_sessions ?? null}
        onLogSessions={(on) => update({ log_sessions: on })}
        disabled={config === null}
        onError={onError}
        onSearch={() => navigate({ group: 'logs', section: 'search' })}
        onScene={() => navigate({ group: 'logs', section: 'scene' })}
      />
      <Section
        id="scrollback"
        title="Scrollback"
        help={{ topic: 'play.scroll-back', subject: 'scrollback' }}
      >
        <Row
          label="Scrollback size"
          description="How many lines you can scroll back through in the terminal. Vosh keeps them for your next launch too."
          anchor="scrollback-size"
        >
          <Select
            value={String(config?.scrollback_lines ?? DEFAULT_SCROLLBACK_LINES)}
            disabled={config === null}
            options={SCROLLBACK_SIZES}
            onChange={(v) => update({ scrollback_lines: Number(v) })}
          />
        </Row>
      </Section>
    </>
  );
}

/** What to call this computer in a sentence: Mac, PC, or computer. */
function computerName(): string {
  const platform =
    typeof document !== 'undefined' ? document.documentElement.dataset.platform : undefined;
  if (platform === 'macos' || (!platform && isMacPlatform())) return 'Mac';
  if (platform === 'windows') return 'PC';
  return 'computer';
}

/** Saved logs with the ways into the scene page and the log view, Log
 *  sessions for the profile Settings shows, and Keep logs for, which
 *  every profile shares since they share one log file. Log
 *  sessions reads the world the selected session dials until you
 *  choose, on for a game and off for this computer. */
function SessionLogsSection({
  logSessions,
  onLogSessions,
  disabled,
  onError,
  onSearch,
  onScene,
}: {
  logSessions: boolean | null;
  onLogSessions: (on: boolean) => void;
  disabled: boolean;
  onError: (message: string | null) => void;
  onSearch: () => void;
  onScene: () => void;
}) {
  const [target] = useSessionTarget();
  const [keep, setKeep] = useState<number | null | undefined>(undefined);
  useEffect(() => {
    let cancelled = false;
    logsKeepGet()
      .then((days) => {
        if (!cancelled) setKeep(days);
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [onError]);

  const pickKeep = async (value: string) => {
    const days = value === 'forever' ? null : Number(value);
    const before = keep;
    setKeep(days);
    try {
      await logsKeepSet(days);
      onError(null);
    } catch (e) {
      setKeep(before);
      onError(String(e));
    }
  };

  return (
    <Section
      id="session-logs"
      title="Session logs"
      help={{ topic: 'characters-and-data.search-logs', subject: 'session logs' }}
    >
      <Row label="Saved logs" description={<SavedLogsCount onError={onError} />}>
        <span className="st-control-group">
          <Button onClick={onScene}>Save a scene…</Button>
          <Button onClick={onSearch}>Search logs…</Button>
        </span>
      </Row>
      <Row
        label="Log sessions"
        description="Saves every line this character's sessions show, so you can search them later. Connections to this computer stay out until you turn it on."
        anchor="log-sessions"
      >
        <Toggle
          checked={logSessions ?? !isLocalHost(target.host)}
          disabled={disabled}
          onChange={onLogSessions}
        />
      </Row>
      <Row
        label="Keep logs for"
        description="Vosh deletes logs older than this once a day. The first time one goes, it rebuilds the log file, which takes a few seconds on a big one. Your game keeps going and the log catches up after. A connect or reconnect waits for it."
        anchor="keep-logs"
      >
        <Select
          value={keep ? String(keep) : 'forever'}
          disabled={keep === undefined}
          options={KEEP_LOGS}
          onChange={(v) => void pickKeep(v)}
        />
      </Row>
    </Section>
  );
}

/** How many logs and lines Vosh saved, leaving out connections to
 *  this machine the way the log view does. A log is one connection,
 *  which the store calls a session. */
function SavedLogsCount({ onError }: { onError: (message: string | null) => void }) {
  const [counts, setCounts] = useState<{ logs: number; lines: number } | null>(null);
  useEffect(() => {
    let cancelled = false;
    listLogSessions(0, { hideLocal: true })
      .then((rows) => {
        if (cancelled) return;
        setCounts({
          logs: rows.length,
          lines: rows.reduce((sum, row) => sum + row.line_count, 0),
        });
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [onError]);
  if (!counts) return 'Counting your saved logs…';
  return savedLogsText(counts.logs, counts.lines, computerName());
}
