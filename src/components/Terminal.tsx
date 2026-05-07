import { useEffect, useRef } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Unicode11Addon } from '@xterm/addon-unicode11';

import '@xterm/xterm/css/xterm.css';
import { onOutput } from '../lib/session';

export interface TerminalHandle {
  write: (data: Uint8Array | string) => void;
  fit: () => void;
  focus: () => void;
  clear: () => void;
}

interface Props {
  onReady?: (handle: TerminalHandle) => void;
}

export function Terminal({ onReady }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XTerm | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const term = new XTerm({
      cursorBlink: true,
      cursorStyle: 'block',
      fontFamily: '"JetBrains Mono", "Fira Code", Menlo, Consolas, ui-monospace, monospace',
      fontSize: 14,
      lineHeight: 1.2,
      scrollback: 10000,
      allowProposedApi: true,
      convertEol: false,
      theme: {
        background: '#0d1117',
        foreground: '#e6edf3',
        cursor: '#e6edf3',
      },
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    const unicode11 = new Unicode11Addon();
    term.loadAddon(unicode11);
    term.unicode.activeVersion = '11';

    term.open(containerRef.current);
    fit.fit();
    termRef.current = term;

    const handleResize = () => {
      try {
        fit.fit();
      } catch {
        // ignore resize before layout settles
      }
    };
    window.addEventListener('resize', handleResize);
    const observer = new ResizeObserver(handleResize);
    observer.observe(containerRef.current);

    let unsubOutput: (() => void) | undefined;
    onOutput((bytes) => term.write(bytes)).then((unlisten) => {
      unsubOutput = unlisten;
    });

    const handle: TerminalHandle = {
      write: (data) => term.write(data),
      fit: () => fit.fit(),
      focus: () => term.focus(),
      clear: () => term.clear(),
    };
    onReady?.(handle);

    return () => {
      observer.disconnect();
      window.removeEventListener('resize', handleResize);
      unsubOutput?.();
      term.dispose();
      termRef.current = null;
    };
  }, [onReady]);

  return <div ref={containerRef} className="terminal-host" />;
}
