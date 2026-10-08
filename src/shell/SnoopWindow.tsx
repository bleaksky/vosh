import { useEffect, useRef } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { onSnoopFind } from '../ipc/snoop';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { isMacPlatform, shortcutKey } from '../lib/shortcuts';
import { useSnoopsOf } from '../stores/session/snoopStore';
import { FindToolbar } from '../terminal/FindToolbar';
import { SnoopTerminal } from '../terminal/SnoopTerminal';
import { WindowControls } from '../ui/WindowControls';
import { SnoopStrip } from './SnoopStrip';
import { useSnoopFind } from './useSnoopFind';
import { useSnoopWindowLook } from './useSnoopWindowLook';

// The snoop window. Open in a window moves every tab of a session here,
// for a second screen. The strip of the split moves up into the 32 band
// beside the traffic lights, as the Settings header sits there, and its
// menu has no Open in a window and no Fold. Under it each tab's
// terminal, the one in front shown, 17 below the band. There is no
// command line: you type in the main window, and Cmd J there brings
// this window forward.
//
// It reads the snoops of its session from the snoop store, whatever
// session the main window shows, every tab with its text fresh from
// snoop_get as it opens. Closing it hands the tabs back: the backend
// clears the session's window flag, and the split shows again with the
// same tabs and text. Once the last tab goes, the window closes itself.

interface Props {
  session: number;
}

export function SnoopWindow({ session }: Props) {
  const mac = isMacPlatform();
  const look = useSnoopWindowLook();
  const snoops = useSnoopsOf(session);
  const { tabs, selected } = snoops;
  const finder = useSnoopFind(session, selected);
  const held = useRef(false);

  // The last tab went, so the window has nothing left to show.
  useEffect(() => {
    if (tabs.length > 0) held.current = true;
    else if (held.current) void getCurrentWindow().close();
  }, [tabs.length]);

  // Cmd F on macOS, Ctrl F elsewhere, and Find in the menu bar, open
  // Find on the tab in front.
  const openFind = finder.open;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey || e.shiftKey) return;
      const mod = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
      if (!mod || shortcutKey(e) !== 'f') return;
      e.preventDefault();
      openFind();
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  }, [mac, openFind]);
  useTauriEvent(onSnoopFind, (to) => {
    if (to === session) openFind();
  });

  return (
    <div className="snoop-window">
      <header className="snoop-window-band" data-tauri-drag-region="">
        <SnoopStrip session={session} snoops={snoops} onFind={openFind} onCaret={() => {}} />
        {!mac && <WindowControls />}
      </header>
      <div className="snoop-body">
        {tabs.map((tab) => (
          <SnoopTerminal
            key={tab.name}
            session={session}
            name={tab.name}
            shown={tab.name === selected}
            fontFamily={look.fontFamily}
            fontSize={look.fontSize}
            lineHeight={look.lineHeight}
            themeTerminalColors={look.themeTerminalColors}
            {...finder.terminal(tab.name)}
          />
        ))}
      </div>
      {finder.finding && (
        <FindToolbar
          key={selected}
          results={finder.results}
          onFindNext={(query, options) => finder.find(query, 'next', options)}
          onFindPrevious={(query, options) => finder.find(query, 'previous', options)}
          onClose={finder.close}
        />
      )}
    </div>
  );
}
