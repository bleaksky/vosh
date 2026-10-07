import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import type { SettingsTarget } from '../../lib/settingsNav';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import type { SettingsPageProps } from '../pageTypes';
import { Button, Segmented, type SegmentedOption } from '../../ui';
import { AliasesEditor } from './AliasesEditor';
import { ImportPanel } from './ImportPanel';
import { LoadoutsEditor } from './LoadoutsEditor';
import { MacrosEditor } from './MacrosEditor';
import { PresetsEditor } from './PresetsEditor';
import { TimersEditor } from './TimersEditor';
import { TriggersEditor } from './TriggersEditor';
import {
  isAutomationKind,
  isListKind,
  type AutomationKind,
  type DirtyReport,
  type TriggersLink,
} from './types';
import { useCloseGuard } from '../useCloseGuard';

// Settings, Automation (the approved SettingsAutomation board). One
// list and detail editor for every kind behind the kind switcher, with
// Import… at the right of the switcher row. Each kind edits a draft,
// and the save bar at the bottom writes it through the kind's existing
// API. Leaving a kind, leaving Automation, or closing the window with
// unsaved changes asks first.
//
// In loadout mode, triggers, aliases, and macros live in the shared
// catalog, and the same API edits it. So does the list of presets that
// are on. Nothing on this page names a character, since those lists
// belong to every character.

const KINDS: readonly SegmentedOption<AutomationKind>[] = [
  { value: 'triggers', label: 'Triggers' },
  { value: 'aliases', label: 'Aliases' },
  { value: 'macros', label: 'Macros' },
  { value: 'timers', label: 'Timers' },
  { value: 'presets', label: 'Presets' },
];

const LOADOUTS: SegmentedOption<AutomationKind> = { value: 'loadouts', label: 'Loadouts' };

type Panel = 'list' | 'json' | 'import';

interface View {
  kind: AutomationKind;
  panel: Panel;
  /** Goes up each time a link asks for the Tick. */
  tickSeq: number;
  /** The preset a link asked for last, by id, and a count that goes up
   *  with each such link. */
  preset: { key: string; seq: number } | null;
  /** The trigger or the filter a preset's card opened Triggers on last. */
  triggers: TriggersLink | null;
}

const START: View = {
  kind: 'triggers',
  panel: 'list',
  tickSeq: 0,
  preset: null,
  triggers: null,
};

const PRESET_ANCHOR = 'presets:';

/** The view a target asks for, or null to stay put. The section names
 *  the kind. The anchors open Import, the JSON view, the Tick, or a
 *  preset as `presets:<id>`. */
function viewFor(target: SettingsTarget, from: View): View | null {
  // A link from a preset's card opens Triggers once, never again on the
  // next visit.
  const current = { ...from, triggers: null };
  const kind = isAutomationKind(target.section) ? target.section : null;
  if (target.anchor === 'import') return { ...current, panel: 'import' };
  if (target.anchor === 'json') {
    const wanted = kind ?? current.kind;
    return { ...current, kind: isListKind(wanted) ? wanted : 'triggers', panel: 'json' };
  }
  if (target.anchor?.startsWith(PRESET_ANCHOR)) {
    const key = target.anchor.slice(PRESET_ANCHOR.length);
    const seq = (current.preset?.seq ?? 0) + 1;
    return { ...current, kind: 'presets', panel: 'list', preset: { key, seq } };
  }
  if (!kind) return null;
  const tickSeq = target.anchor === 'tick' ? current.tickSeq + 1 : current.tickSeq;
  return { ...current, kind, panel: 'list', tickSeq };
}

/** Whether moving from one view to the next drops the current draft. */
function leavesDraft(from: View, to: View): boolean {
  if (from.panel === 'import') return false;
  if (to.panel === 'import') return true;
  return from.kind !== to.kind;
}

export function AutomationPage({
  target,
  navSeq,
  config,
  setConfig,
  onError,
  pathB,
  setLeaveGuard,
}: SettingsPageProps) {
  const [view, setViewState] = useState<View>(() => viewFor(target, START) ?? START);
  const [dirty, setDirty] = useState<DirtyReport | null>(null);
  const [confirm, setConfirm] = useState<{ report: DirtyReport; proceed: () => void } | null>(null);
  const viewRef = useRef(view);
  const dirtyRef = useRef<DirtyReport | null>(null);

  const setView = useCallback((next: View) => {
    viewRef.current = next;
    setViewState(next);
  }, []);

  const onDirty = useCallback((report: DirtyReport | null) => {
    dirtyRef.current = report;
    setDirty(report);
  }, []);

  /** Run `proceed` now, or after you agree to drop unsaved changes. */
  const ask = useCallback((proceed: () => void) => {
    const report = dirtyRef.current;
    if (!report) {
      proceed();
      return;
    }
    setConfirm({ report, proceed });
  }, []);

  const go = useCallback(
    (next: View) => {
      if (leavesDraft(viewRef.current, next)) ask(() => setView(next));
      else setView(next);
    },
    [ask, setView],
  );

  // A nav press, search hit, or deep link. The first target already
  // set the starting view.
  const firstNav = useRef(true);
  useEffect(() => {
    if (firstNav.current) {
      firstNav.current = false;
      return;
    }
    const next = viewFor(target, viewRef.current);
    if (next) go(next);
    // navSeq marks each navigation, even to the same target.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navSeq]);

  // Moving to another group asks too.
  useEffect(() => {
    setLeaveGuard((proceed) => {
      if (!dirtyRef.current) return false;
      ask(proceed);
      return true;
    });
    return () => setLeaveGuard(null);
  }, [setLeaveGuard, ask]);

  // So does closing the window.
  useCloseGuard(dirty !== null, ask);

  const kinds = pathB ? [...KINDS, LOADOUTS] : KINDS;
  const openTriggers = useCallback(
    (to: Omit<TriggersLink, 'seq'>) => {
      const current = viewRef.current;
      const seq = (current.triggers?.seq ?? 0) + 1;
      go({ ...current, kind: 'triggers', panel: 'list', triggers: { ...to, seq } });
    },
    [go],
  );
  const openPreset = useCallback(
    (key: string) => {
      const current = viewRef.current;
      const seq = (current.preset?.seq ?? 0) + 1;
      go({ ...current, kind: 'presets', panel: 'list', preset: { key, seq }, triggers: null });
    },
    [go],
  );
  const openJson = useCallback(
    (open: boolean) => setView({ ...viewRef.current, panel: open ? 'json' : 'list' }),
    [setView],
  );

  let body: ReactNode = null;
  if (view.panel === 'import') {
    body = (
      <div className="st-auto-body st-auto-scrollbody">
        <ImportPanel onError={onError} />
      </div>
    );
  } else {
    const json = view.panel === 'json';
    switch (view.kind) {
      case 'triggers':
        body = (
          <TriggersEditor
            key="triggers"
            json={json}
            onJson={openJson}
            onDirty={onDirty}
            onError={onError}
            open={view.triggers}
            onOpenPreset={openPreset}
          />
        );
        break;
      case 'aliases':
        body = (
          <AliasesEditor
            key="aliases"
            json={json}
            onJson={openJson}
            onDirty={onDirty}
            onError={onError}
          />
        );
        break;
      case 'macros':
        body = (
          <MacrosEditor
            key="macros"
            json={json}
            onJson={openJson}
            onDirty={onDirty}
            onError={onError}
          />
        );
        break;
      case 'timers':
        body = (
          <TimersEditor
            key="timers"
            json={json}
            onJson={openJson}
            onDirty={onDirty}
            onError={onError}
            tickSeq={view.tickSeq}
          />
        );
        break;
      case 'presets':
        body = config ? (
          <PresetsEditor
            key="presets"
            config={config}
            setConfig={setConfig}
            pathB={pathB}
            onDirty={onDirty}
            onError={onError}
            selectPreset={view.preset}
            onOpenTriggers={openTriggers}
          />
        ) : null;
        break;
      case 'loadouts':
        body = pathB ? (
          <LoadoutsEditor key="loadouts" onDirty={onDirty} onError={onError} />
        ) : (
          <p className="st-auto-empty">
            Loadouts work in loadout mode. Open Import… and preview a shared catalog to set it up.
          </p>
        );
        break;
    }
  }

  return (
    <div className="st-auto">
      <div className="st-toolbar">
        <Segmented
          label="Kind"
          options={kinds}
          value={view.panel === 'import' ? null : view.kind}
          onChange={(kind) => go({ ...viewRef.current, kind, panel: 'list', triggers: null })}
        />
        <Button onClick={() => go({ ...viewRef.current, panel: 'import' })}>Import…</Button>
      </div>
      {body}
      {confirm && (
        <ConfirmDialog
          title={confirm.report.title}
          body={confirm.report.body}
          confirmLabel="Discard"
          onConfirm={() => {
            const proceed = confirm.proceed;
            setConfirm(null);
            proceed();
          }}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  );
}
