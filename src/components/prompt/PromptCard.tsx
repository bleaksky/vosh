import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { BAND_OUTSET_Y, DOCK_GAP, type CellSize } from '../../lib/promptBand';
import {
  cardAnchor,
  codesSourceLine,
  headerButtons,
  localStamp,
  moreItems,
  openingStep,
  savedCapture,
  savedForName,
  type CardStep,
  type MoreItemId,
} from '../../lib/promptCard';
import type { PromptShowState } from '../../lib/promptShow';
import {
  onPromptState,
  profilesList,
  promptCardOpen,
  promptCompile,
  promptConfigGet,
  promptConfigSet,
  promptDesignsList,
  promptPreviewSet,
  promptStateGet,
  promptWatch,
  sessionIdentityGet,
  subscribePromptConfigChanged,
  type PromptCompileReport,
  type PromptConfig,
  type PromptDesign,
  type PromptPreset,
  type PromptPreviewName,
  type PromptState,
  type SessionIdentity,
} from '../../lib/session';
import { openSettingsTab } from '../../lib/settingsLink';
import { useEscape } from '../../lib/escapeStack';
import { useGamePrompt } from '../../lib/stores/gamePromptStore';
import { pushToast } from '../../lib/toasts';
import { useBandEnv } from '../../lib/useBandEnv';
import { useCellWidth, useLabelMeasure } from '../../lib/useCellWidth';
import { knownWorld } from '../../lib/useConnection';
import { ConfirmDialog } from '../ConfirmDialog';
import type { TerminalHandle } from '../Terminal';
import {
  Button,
  CloseIcon,
  IconButton,
  MoreIcon,
  Segmented,
  Toggle,
  type SegmentedOption,
} from '../settings/ui';
import { CardMenu, MenuSeparator } from './CardMenu';
import { CodesEntry, CodesRead, type CodesRequest } from './PromptCodes';
import { PointName, PointPick, type PointedLine } from './PromptPoint';
import { DrawOff, Starts } from './PromptStarts';

// The prompt card (section 7 of the prompt build spec). It opens from the
// terminal menu on any row, the palette, or Customize… in Settings, over
// your prompt: 4 px above the row right above it in the text and lifted,
// 4 px above the band while it is pinned, and over the last row while no
// prompt is open. It is saved for the character that owns the profile.
//
// With no capture it walks you through the capture steps: the codes the
// game sent (P3), your setting when the game sent none (P2), or the line
// another game prints (P15). Then it offers designs to start from (P4),
// and on every later open it rests with the Presets menu. Every change
// saves as you make it, and Command Z takes the last one back. While it
// reads your codes your prompt shows the line the game sent, and once it
// draws your design it labels each value with nothing to show, so you
// can point at it. Closing it puts your live prompt back.

/** Where the card reaches the terminal it sits over. */
export interface PromptCardHost {
  terminal: () => TerminalHandle | null;
  /** The terminal area, which the card keeps inside. */
  area: () => HTMLElement | null;
  /** The pinned band's dock, while your prompt shows pinned. */
  dock: () => HTMLElement | null;
}

interface PromptCardProps {
  host: PromptCardHost;
  show: PromptShowState | null;
  cell: CellSize | null;
  /** The terminal's face, which the card sets prompts in. */
  monoFamily: string;
  themeTerminalColors: boolean;
  brightBold: boolean;
  renderer: 'xterm' | 'native';
  onClose: () => void;
}

/** The previews the footer offers. Lament only under the Forsaken Lands
 *  rules, where the game hides your values under lamented tears. */
function previewOptions(forsaken: boolean): SegmentedOption<PromptPreviewName>[] {
  const options: SegmentedOption<PromptPreviewName>[] = [
    { value: 'now', label: 'Now' },
    { value: 'low_health', label: 'Low health' },
    { value: 'fight', label: 'Fight' },
  ];
  if (forsaken) options.push({ value: 'lament', label: 'Lament' });
  return options;
}

const UNDO_DEPTH = 50;

export function PromptCard({
  host,
  show,
  cell,
  monoFamily,
  themeTerminalColors,
  brightBold,
  renderer,
  onClose,
}: PromptCardProps) {
  const drawId = useId();
  const cardRef = useRef<HTMLDivElement | null>(null);
  const [config, setConfig] = useState<PromptConfig | null>(null);
  const [state, setState] = useState<PromptState | null>(null);
  const [identity, setIdentity] = useState<SessionIdentity | null>(null);
  const [active, setActive] = useState('default');
  const [knownHost, setKnownHost] = useState(false);
  const [step, setStep] = useState<CardStep | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [preview, setPreview] = useState<PromptPreviewName>('now');
  const [moreAt, setMoreAt] = useState<HTMLElement | null>(null);
  const [confirmForget, setConfirmForget] = useState(false);
  // The code reader for another game, once you choose it in More.
  const [codesChosen, setCodesChosen] = useState(false);
  const [entryCodes, setEntryCodes] = useState<{ prompt: string; fprompt: string } | null>(null);
  const [request, setRequest] = useState<CodesRequest | null>(null);
  const [pointed, setPointed] = useState<PointedLine | null>(null);
  const [pickFrom, setPickFrom] = useState(0);
  const [presets, setPresets] = useState<PromptPreset[]>([]);
  const [designs, setDesigns] = useState<PromptDesign[]>([]);
  const [anchor, setAnchor] = useState<{ left: number; bottom: number; maxHeight: number } | null>(
    null,
  );
  const undo = useRef<PromptConfig[]>([]);
  const game = useGamePrompt();
  const env = useBandEnv(themeTerminalColors, brightBold, renderer);
  const cellW = useCellWidth(monoFamily);
  const measure = useLabelMeasure(11);

  const forsaken =
    codesChosen || (state?.forsaken ?? false) || knownHost || config?.capture.kind === 'aabahran';
  const gameSent = (state?.new_build ?? false) || game !== null;

  // Open: keep the design among the earlier ones, read the state, and
  // name who the card saves for.
  useEffect(() => {
    let alive = true;
    void Promise.all([
      promptCardOpen(),
      promptStateGet(),
      sessionIdentityGet().catch(() => null),
      profilesList().catch(() => null),
    ])
      .then(([opened, now, who, list]) => {
        if (!alive) return;
        const name = list?.active ?? who?.profile ?? 'default';
        const entry = list?.profiles.find((p) => p.name === name);
        const host = who?.host ?? entry?.auto_match?.host ?? '';
        const known = knownWorld(host) !== undefined;
        setConfig(opened);
        setState(now);
        setIdentity(who);
        setActive(name);
        setKnownHost(known);
        setStep(
          openingStep({
            capture: opened.capture,
            forsaken: now.forsaken || known || opened.capture.kind === 'aabahran',
            gameSent: now.new_build,
          }),
        );
      })
      .catch((e: unknown) => console.error('[prompt card] opening failed', e));
    void promptWatch(true).catch(() => {});
    const unlisteners: (() => void)[] = [];
    const keep = (p: Promise<() => void>) =>
      void p.then((fn) => (alive ? unlisteners.push(fn) : fn())).catch(() => {});
    keep(
      onPromptState((next) => {
        setState(next);
        setRefresh((n) => n + 1);
      }),
    );
    keep(
      subscribePromptConfigChanged(() => {
        void promptConfigGet()
          .then((next) => {
            if (alive) setConfig(next);
          })
          .catch(() => {});
      }),
    );
    return () => {
      alive = false;
      for (const fn of unlisteners) fn();
      void promptWatch(false).catch(() => {});
      // Your live prompt comes back as the card closes.
      void promptPreviewSet(null).catch(() => {});
    };
  }, []);

  // While the card reads your codes your prompt shows the line the game
  // sent, so its marks sit on it. Once it draws your design, each value
  // with nothing to show draws its label, and the footer's preview runs.
  const reading = step === 'codes-entry' || step === 'codes' || step === 'point' || step === 'name';
  useEffect(() => {
    if (step === null) return;
    void promptPreviewSet(
      reading ? { raw: true } : { placeholders: true, preview: preview === 'now' ? null : preview },
    ).catch(() => {});
  }, [step, reading, preview]);

  // The designs to start from, for the capture the profile holds.
  const capture = config?.capture;
  useEffect(() => {
    if (!capture || (step !== 'start' && step !== 'rest')) return;
    let alive = true;
    const request =
      capture.kind === 'aabahran'
        ? promptCompile({ kind: 'aabahran', prompt: capture.prompt, fprompt: capture.fprompt })
        : promptCompile({
            kind: 'regex',
            lines: capture.kind === 'regex' ? capture.lines : [],
            names: capture.kind === 'regex' ? (capture.names ?? {}) : {},
          });
    void Promise.all([request, promptDesignsList().catch(() => [])])
      .then(([report, others]) => {
        if (!alive) return;
        setPresets(report.presets);
        setDesigns(others);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [capture, step]);

  // Sit over your prompt, and follow it.
  const relayout = useCallback(async () => {
    const area = host.area();
    if (!area) return;
    const rect = area.getBoundingClientRect();
    const cellH = cell?.height ?? 17.5;
    const term = host.terminal();
    let lastRowTop = rect.bottom - cellH;
    let promptTop: number | null = null;
    if (term) {
      const rows = term.getSize().rows;
      lastRowTop = term.rowTop(rows - 1) ?? lastRowTop;
      const region = await term.promptRegion().catch(() => null);
      if (region && region.atBottom) {
        const top = term.rowTop(region.row);
        if (top !== null && top >= rect.top) promptTop = top;
      }
    }
    const pinned = show?.show === 'pinned' && show.capture;
    const dock = pinned ? host.dock() : null;
    const bandRowTop = dock ? dock.getBoundingClientRect().top + DOCK_GAP + BAND_OUTSET_Y : null;
    const placed = cardAnchor({
      pinned: Boolean(pinned),
      promptTop,
      lastRowTop,
      bandRowTop,
      cellH,
      areaTop: rect.top,
      viewportH: window.innerHeight,
    });
    setAnchor({ left: rect.left + 12, ...placed });
  }, [host, cell, show]);

  useLayoutEffect(() => {
    void relayout();
  }, [relayout, step, refresh]);

  useEffect(() => {
    const onResize = () => void relayout();
    window.addEventListener('resize', onResize);
    const area = host.area();
    const observer = area ? new ResizeObserver(onResize) : null;
    if (area) observer?.observe(area);
    return () => {
      window.removeEventListener('resize', onResize);
      observer?.disconnect();
    };
  }, [host, relayout]);

  // Focus moves into the card so the keyboard reaches it.
  useEffect(() => {
    if (step !== null) cardRef.current?.focus({ preventScroll: true });
  }, [step === null]); // eslint-disable-line react-hooks/exhaustive-deps

  useEscape(true, onClose);

  const save = (next: PromptConfig, keepUndo = true) => {
    if (!config) return;
    if (keepUndo) undo.current = [...undo.current, config].slice(-UNDO_DEPTH);
    setConfig(next);
    void promptConfigSet(next).catch((e: unknown) => {
      setConfig(config);
      pushToast({ kind: 'error', message: String(e) });
    });
  };

  const takeBack = () => {
    const last = undo.current.pop();
    if (last) save(last, false);
  };

  const more =
    config && step ? moreItems({ step, forsaken, gameSent, capture: config.capture }) : [];
  const buttons = step ? headerButtons(step, more) : { editAsText: false, more: false };
  const saved = savedForName(identity, active);
  const owner = saved.replace(/^Saved for /, '');

  const runMore = (id: MoreItemId) => {
    setMoreAt(null);
    switch (id) {
      case 'change-codes':
        setEntryCodes(
          config?.capture.kind === 'aabahran'
            ? { prompt: config.capture.prompt, fprompt: config.capture.fprompt }
            : null,
        );
        setStep('codes-entry');
        return;
      case 'point':
        setStep('point');
        return;
      case 'use-codes':
        setCodesChosen(true);
        setEntryCodes(null);
        setStep('codes-entry');
        return;
      case 'forget':
        setConfirmForget(true);
        return;
    }
  };

  // The codes P3 reads: the ones the game sent on the new build, or the
  // ones you told Vosh on P2.
  const codes: CodesRequest | null =
    gameSent && game && !request
      ? {
          prompt: game.prompt,
          fprompt: game.fprompt,
          typed: false,
          source: 'gmcp',
          seenAt: localStamp(new Date(game.receivedAt)),
        }
      : request;

  const useCodes = (report: PromptCompileReport) => {
    if (!config || !codes) return;
    save({ ...config, capture: savedCapture(report, codes.source, codes.seenAt) });
    setStep('start');
  };

  const useNames = (report: PromptCompileReport) => {
    const shape = report.shapes[0];
    if (!config || !shape) return;
    save({
      ...config,
      capture: {
        kind: 'regex',
        lines: shape.lines,
        settle: shape.settle,
        names: report.names,
        seen_at: localStamp(new Date()),
        source: 'session',
      },
    });
    setStep('start');
  };

  const forget = () => {
    setConfirmForget(false);
    if (!config) return;
    const next: PromptConfig = { ...config, capture: { kind: 'none' } };
    save(next);
    setRequest(null);
    setStep(openingStep({ capture: next.capture, forsaken, gameSent }));
  };

  // Interim until the card's own Edit as text and picker land: the
  // design's text in Settings, under Input, then Advanced.
  const editAsText = () => openSettingsTab('input:advanced#prompt');

  let body: ReactNode = null;
  if (config && step) {
    switch (step) {
      case 'codes-entry':
        body = (
          <CodesEntry
            key="codes-entry"
            initial={entryCodes}
            onRead={(next) => {
              setRequest(next);
              setStep('codes');
            }}
            onPoint={() => setStep('point')}
            onGameSent={() => {
              setRequest(null);
              setStep('codes');
            }}
          />
        );
        break;
      case 'codes':
        body = codes ? (
          <CodesRead
            request={codes}
            sourceLine={codes.source === 'gmcp' ? codesSourceLine(game) : null}
            capture={config.capture}
            secondary={
              codes.source === 'gmcp'
                ? { label: 'Point at the line instead', onClick: () => setStep('point') }
                : {
                    label: 'Change codes',
                    onClick: () => {
                      setEntryCodes({ prompt: codes.prompt, fprompt: codes.fprompt });
                      setStep('codes-entry');
                    },
                  }
            }
            onUse={useCodes}
            refresh={refresh}
            env={env}
            cellW={cellW}
            measure={measure}
          />
        ) : null;
        break;
      case 'point':
        body = (
          <PointPick
            start={pickFrom}
            onRead={(line, group) => {
              setPointed(line);
              setPickFrom(group);
              setStep('name');
            }}
          />
        );
        break;
      case 'name':
        body = pointed ? (
          <PointName
            line={pointed}
            onUse={useNames}
            onPickAnother={() => {
              setPickFrom((g) => g + 1);
              setStep('point');
            }}
            refresh={refresh}
            env={env}
            cellW={cellW}
          />
        ) : null;
        break;
      case 'start':
      case 'rest': {
        const drawOff = !config.draw && step === 'rest';
        body = (
          <>
            {drawOff ? (
              <DrawOff
                name={owner}
                other={!forsaken}
                confirming={confirmForget}
                onForget={() => setConfirmForget(true)}
              />
            ) : (
              <Starts
                mode={step}
                config={config}
                presets={presets}
                designs={designs}
                values={(state?.packages.length ?? 0) > 0 ? 'live' : 'sample'}
                refresh={refresh}
                env={env}
                cellW={cellW}
                onPick={(template) => save({ ...config, template, draw: true })}
                onInsertValue={editAsText}
              />
            )}
            <div className="pc-rule" aria-hidden="true" />
            <div className="pc-foot">
              <Toggle
                id={drawId}
                checked={config.draw}
                onChange={(draw) => save({ ...config, draw })}
              />
              <label className="pc-switch" htmlFor={drawId}>
                Draw your prompt
              </label>
              <span className="pc-spacer" />
              {config.draw && (
                <Segmented
                  label="Preview"
                  options={previewOptions(forsaken)}
                  value={preview}
                  onChange={setPreview}
                />
              )}
              <Button variant="primary" className="pc-done" onClick={onClose}>
                Done
              </Button>
            </div>
          </>
        );
        break;
      }
    }
  }

  return (
    <>
      <div
        ref={cardRef}
        className="pc-card st-controls"
        role="dialog"
        aria-label="Customize prompt"
        tabIndex={-1}
        data-occludes-surface="true"
        style={{
          left: anchor?.left ?? 12,
          bottom: anchor?.bottom ?? 0,
          maxHeight: anchor?.maxHeight,
          visibility: anchor && step ? 'visible' : 'hidden',
        }}
        onMouseUp={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          const mod = e.metaKey || e.ctrlKey;
          const field =
            e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
          if (mod && !e.shiftKey && e.key.toLowerCase() === 'z' && !field) {
            e.preventDefault();
            takeBack();
          }
        }}
      >
        <div className="pc-head">
          <h2 className="pc-title">Customize prompt</h2>
          <span className="pc-saved">{saved}</span>
          <span className="pc-spacer" />
          {buttons.editAsText && <Button onClick={editAsText}>Edit as text</Button>}
          {buttons.more && (
            <IconButton
              label="Prompt options"
              icon={<MoreIcon />}
              aria-haspopup="menu"
              aria-expanded={moreAt !== null}
              onClick={(e) => setMoreAt(moreAt ? null : e.currentTarget)}
            />
          )}
          <IconButton label="Close" icon={<CloseIcon />} onClick={onClose} />
        </div>
        <div className="pc-rule" aria-hidden="true" />
        {body}
      </div>
      {moreAt && more.length > 0 && (
        <CardMenu
          anchor={moreAt}
          place="below-end"
          width={forsaken ? 232 : 264}
          label="Prompt options"
          onClose={() => setMoreAt(null)}
        >
          {more.map((item, i) =>
            item === 'separator' ? (
              <MenuSeparator key={`sep-${i}`} />
            ) : (
              <li key={item.id} role="none">
                <button
                  type="button"
                  role="menuitem"
                  className={`ov-menu-item${item.danger ? ' is-danger' : ''}`}
                  onClick={() => runMore(item.id)}
                >
                  <span className="ov-menu-label">{item.label}</span>
                </button>
              </li>
            ),
          )}
        </CardMenu>
      )}
      {confirmForget && (
        <ConfirmDialog
          title="Forget your game's prompt?"
          body={`Vosh stops reading your prompt for ${owner}, and the game's own prompt shows again. Your design stays saved.`}
          confirmLabel="Forget"
          onConfirm={forget}
          onCancel={() => setConfirmForget(false)}
        />
      )}
    </>
  );
}
