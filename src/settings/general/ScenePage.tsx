import { Fragment, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import {
  listLogSessions,
  previewScene,
  saveScene,
  type LogSession,
  type SceneFilter,
  type SceneFormat,
  type ScenePreview,
  type SceneRange,
} from '../../ipc/logs';
import { useSessionTarget } from '../../stores/session/useConnection';
import { getCurrentThemeId } from '../../theme/theme';
import { findTheme, resolveThemeTerminalColors } from '../../theme/themes';
import type { SettingsPageProps } from '../pageTypes';
import {
  Button,
  Card,
  Chip,
  ChipButton,
  Field,
  PlusIcon,
  Row,
  Segmented,
  Select,
  Toggle,
} from '../../ui';
import { MenuItem, MenuSurface, type MenuPlacement } from '../../ui/MenuSurface';
import { menuBelow } from '../../ui/menuPlacement';
import {
  logPalette,
  logSessionLabel,
  logSpanCss,
  logTime,
  parseLogLine,
  savedPalette,
} from './logView';
import {
  addable,
  clockText,
  endingAt,
  FIRST_FILTER,
  firstRange,
  keptText,
  logsWorld,
  startingAt,
  withFrom,
  withTo,
} from './scene';

// Save a scene (board 5 of the Alerts and Scenes review), a page inside
// General at general:scene. The toolbar picks the log, a From and a To on
// the log's own 24 hour clock, and the format. Prompts, Your commands
// and Channels left out say what the scene leaves out (Q10), and the
// preview shows every line in the range, what stays out drawn quiet with
// the reason beside it. A click on a time starts the scene on that line,
// and a Shift click ends it there. Save scene writes the file to
// Downloads (Q12), and the main window says so with a button that shows
// it. Cancel goes back to Session logs.

const FORMATS: readonly { value: SceneFormat; label: string }[] = [
  { value: 'text', label: 'Text' },
  { value: 'ansi', label: 'ANSI' },
  { value: 'html', label: 'HTML' },
];

/** Wait this long after a change before reading the preview again. */
const PREVIEW_DELAY_MS = 120;

interface Props extends SettingsPageProps {
  /** The log Save a scene… in Session logs picked, or null to open on
   *  the newest log of the selected session. */
  log: number | null;
}

type Status = { kind: 'ready' } | { kind: 'saving' } | { kind: 'saved'; name: string };

export function ScenePage({ config, onError, navigate, log: picked }: Props) {
  const [target] = useSessionTarget();
  const { host, port } = target;
  const [logs, setLogs] = useState<LogSession[]>([]);
  const [range, setRange] = useState<SceneRange | null>(null);
  const [filter, setFilter] = useState<SceneFilter>(FIRST_FILTER);
  const [format, setFormat] = useState<SceneFormat>('html');
  const [preview, setPreview] = useState<ScenePreview | null>(null);
  const [status, setStatus] = useState<Status>({ kind: 'ready' });
  const [addMenu, setAddMenu] = useState<{ at: MenuPlacement; anchor: HTMLElement } | null>(null);
  const [fromText, setFromText] = useState('');
  const [toText, setToText] = useState('');
  const seq = useRef(0);
  const pageRef = useRef<HTMLDivElement | null>(null);
  const logged = logsWorld(config?.log_sessions ?? null, host);

  // The logs of the world the selected session dials, and the range the
  // page opens on: the log Session logs picked, or this session's newest.
  useEffect(() => {
    let cancelled = false;
    Promise.all([
      listLogSessions(0, { host, port }),
      picked === null ? listLogSessions(1, { thisSession: true }) : Promise.resolve([]),
    ])
      .then(([world, mine]) => {
        if (cancelled) return;
        setLogs(world);
        const first = world.find((s) => s.id === picked) ?? mine[0] ?? world[0] ?? null;
        if (first) setRange(firstRange(first));
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [host, port, picked, onError]);

  const log = logs.find((s) => s.id === range?.log) ?? null;

  // A change after a save makes a new scene to save.
  useEffect(() => setStatus({ kind: 'ready' }), [range, filter, format]);

  useEffect(() => {
    if (!range) return;
    setFromText(clockText(range.fromMs));
    setToText(clockText(range.toMs));
  }, [range]);

  // Read the preview again as the range, the filter or the format change.
  useEffect(() => {
    if (!range) return;
    const mine = ++seq.current;
    const timer = window.setTimeout(() => {
      previewScene(range, filter, format)
        .then((next) => {
          if (mine === seq.current) setPreview(next);
        })
        .catch((e) => {
          if (mine === seq.current) onError(String(e));
        });
    }, PREVIEW_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [range, filter, format, onError]);

  // The lines in their own colors, as the log view draws them.
  const themeId = getCurrentThemeId();
  const themeColors = resolveThemeTerminalColors(config?.theme_terminal_colors ?? null);
  const baseAnsi = config?.terminal_base_ansi ?? null;
  const brightBold = config?.bright_bold ?? false;
  const sixteen = useMemo(
    () => logPalette(findTheme(themeId).xterm, themeColors, baseAnsi),
    [themeId, themeColors, baseAnsi],
  );
  const lines = useMemo(
    () =>
      (preview?.lines ?? []).map((line) => ({
        ...line,
        spans: parseLogLine(line.raw && line.raw.length > 0 ? new Uint8Array(line.raw) : line.text),
      })),
    [preview],
  );

  const commitFrom = () => {
    const next = range && log ? withFrom(range, log, fromText) : null;
    if (next) setRange(next);
    else if (range) setFromText(clockText(range.fromMs));
  };
  const commitTo = () => {
    const next = range ? withTo(range, toText) : null;
    if (next) setRange(next);
    else if (range) setToText(clockText(range.toMs));
  };

  const pickTime = (event: MouseEvent, id: number, ts: number) => {
    if (!range) return;
    setRange(event.shiftKey ? endingAt(range, id, ts) : startingAt(range, id, ts));
  };

  const save = async () => {
    if (!range) return;
    setStatus({ kind: 'saving' });
    try {
      const name = await saveScene(
        range,
        filter,
        format,
        format === 'html' ? savedPalette(sixteen, findTheme(themeId).xterm, pageRef.current) : null,
      );
      setStatus({ kind: 'saved', name });
    } catch (e) {
      setStatus({ kind: 'ready' });
      onError(String(e));
    }
  };

  const leaveOut = (name: string) => {
    setAddMenu(null);
    setFilter((f) => ({ ...f, leftOut: [...f.leftOut, name] }));
  };
  const keep = (name: string) =>
    setFilter((f) => ({ ...f, leftOut: f.leftOut.filter((n) => n !== name) }));

  const statusText = (() => {
    if (!logged) return 'Logging is off for this profile, so there is nothing to save.';
    if (!range) return logs.length === 0 ? 'Vosh has no log of this world to save from.' : '';
    switch (status.kind) {
      case 'saving':
        return 'Saving…';
      case 'saved':
        return `Saved ${status.name} in Downloads`;
      case 'ready':
        return preview ? `Saves ${preview.file_name} to Downloads` : '';
    }
  })();

  const logOptions = logs.map((s) => ({
    value: String(s.id),
    label: logSessionLabel(s.started_at_ms),
  }));
  const canSave = logged && range !== null && (preview?.kept ?? 0) > 0 && status.kind !== 'saving';

  return (
    <div className="st-auto sc-page" ref={pageRef}>
      <div className="st-toolbar">
        <div className="sc-range">
          <Select
            aria-label="Log"
            width={140}
            value={range ? String(range.log) : ''}
            options={logOptions}
            disabled={logs.length === 0}
            onChange={(value) => {
              const next = logs.find((s) => String(s.id) === value);
              if (next) setRange(firstRange(next));
            }}
          />
          <Field
            className="sc-time"
            width={58}
            aria-label="From"
            value={fromText}
            disabled={!range}
            onChange={setFromText}
            onBlur={commitFrom}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commitFrom();
            }}
          />
          <span className="sc-to">to</span>
          <Field
            className="sc-time"
            width={58}
            aria-label="To"
            value={toText}
            disabled={!range}
            onChange={setToText}
            onBlur={commitTo}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commitTo();
            }}
          />
        </div>
        <Segmented label="Format" options={FORMATS} value={format} onChange={setFormat} />
      </div>
      <div className="st-auto-body sc-body">
        <Card className="st-auto-card sc-opts">
          <Row label="Prompts">
            <Toggle
              checked={filter.prompts}
              onChange={(on) => setFilter((f) => ({ ...f, prompts: on }))}
            />
          </Row>
          <Row label="Your commands" className="sc-opt-right">
            <Toggle
              checked={filter.commands}
              onChange={(on) => setFilter((f) => ({ ...f, commands: on }))}
            />
          </Row>
          <Row label="Channels left out" className="sc-opt-wide">
            <ul className="st-chips">
              {filter.leftOut.map((name) => (
                <Chip key={name} as="li" onRemove={() => keep(name)} removeLabel={`Keep ${name}`}>
                  {name}
                </Chip>
              ))}
              <li>
                <ChipButton
                  icon={<PlusIcon size={12} />}
                  disabled={addable(filter.leftOut).length === 0}
                  aria-haspopup="menu"
                  aria-expanded={addMenu !== null}
                  onClick={(e) => {
                    if (addMenu) setAddMenu(null);
                    else
                      setAddMenu({
                        at: menuBelow(e.currentTarget.getBoundingClientRect()),
                        anchor: e.currentTarget,
                      });
                  }}
                >
                  Add
                </ChipButton>
              </li>
            </ul>
          </Row>
        </Card>
        {addMenu && (
          <MenuSurface
            label="Leave out a channel"
            at={addMenu.at}
            anchor={addMenu.anchor}
            onClose={() => setAddMenu(null)}
          >
            {addable(filter.leftOut).map((name) => (
              <MenuItem key={name} onSelect={() => leaveOut(name)}>
                {name}
              </MenuItem>
            ))}
          </MenuSurface>
        )}
        <div className="sc-prev">
          <div className="st-section-head">
            <h2 className="st-section-title">Preview</h2>
            <div className="st-section-actions">
              <span className="st-meta">
                {preview ? keptText(preview.kept, preview.total) : ''}
              </span>
            </div>
          </div>
          {preview?.older && (
            <p className="sc-note">
              Some of these lines are older than Save a scene, so Vosh found the prompts and
              channels by their text.
            </p>
          )}
          <ul className="st-logs-lines sc-lines" aria-label="Preview">
            {lines.map((line) => (
              <li
                key={line.id}
                className={line.out === null ? 'st-logs-line' : 'st-logs-line is-out'}
              >
                <button
                  type="button"
                  className="st-logs-time sc-time-pick"
                  title="Start here. Shift click to end here."
                  onClick={(e) => pickTime(e, line.id, line.ts_ms)}
                >
                  {logTime(line.ts_ms)}
                </button>
                <span className="st-logs-text">
                  {line.spans.map((span, i) => {
                    const css = logSpanCss(span, sixteen, brightBold);
                    return Object.keys(css).length > 0 ? (
                      <span key={i} style={css}>
                        {span.text}
                      </span>
                    ) : (
                      <Fragment key={i}>{span.text}</Fragment>
                    );
                  })}
                </span>
                {line.out && <span className="sc-why">{line.out}</span>}
              </li>
            ))}
          </ul>
          {preview?.capped && (
            <p className="sc-note">The preview stops here. The scene keeps every line.</p>
          )}
        </div>
      </div>
      <div className="st-savebar">
        <div className="st-savebar-side">
          <span className="st-savebar-status" role="status" aria-live="polite">
            {statusText}
          </span>
        </div>
        <div className="st-savebar-actions">
          <Button onClick={() => navigate({ group: 'general', section: 'logs' })}>Cancel</Button>
          <Button variant="primary" disabled={!canSave} onClick={() => void save()}>
            Save scene
          </Button>
        </div>
      </div>
    </div>
  );
}
