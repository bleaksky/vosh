import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { forEachDiagnostic } from '@codemirror/lint';
import { EditorState, type TransactionSpec } from '@uiw/react-codemirror';
import { FakeDocument } from '../test/fakeDom';
import type { CodeEditor as CodeEditorType } from './CodeEditor';
import type { CodeMark } from './codeEditorStyle';

// The shared editor, mounted in the stand in for the DOM in
// src/test/fakeDom.ts. CodeMirror itself needs a real DOM, so its React
// wrapper is a stand in that keeps the props it gets, and a test hands
// the editor a view over a real editor state.

const seen = vi.hoisted(() => ({ props: [] as Record<string, unknown>[] }));

vi.mock('@uiw/react-codemirror', async (actual) => ({
  ...(await actual<typeof import('@uiw/react-codemirror')>()),
  default: (props: Record<string, unknown>) => {
    seen.props.push(props);
    return null;
  },
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let CodeEditor: typeof CodeEditorType;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node' });
  ({ createRoot } = await import('react-dom/client'));
  ({ CodeEditor } = await import('./CodeEditor'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const CODE =
  'mud.on_gmcp("Char.Vitals", function(data)\n  while data.hp < data.maxhp do\n  end\nend)\n';
const STOP: CodeMark = {
  line: 2,
  message: 'Vosh stopped wait_full at main.lua line 5 after 100 ms.',
};

/** A view over a real editor state that takes what the editor sends. */
function fakeView() {
  const view = {
    state: EditorState.create({ doc: CODE }),
    sent: 0,
    dispatch(spec: TransactionSpec) {
      view.sent += 1;
      view.state = view.state.update(spec).state;
    },
  };
  return view;
}

/** The text each mark covers in `state`. */
function marked(state: EditorState): string[] {
  const out: string[] = [];
  forEachDiagnostic(state, (_d, from, to) => void out.push(state.sliceDoc(from, to)));
  return out;
}

async function mount(props: Partial<Parameters<typeof CodeEditorType>[0]>) {
  seen.props.length = 0;
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const draw = async (next: Partial<Parameters<typeof CodeEditorType>[0]>) => {
    await act(async () => {
      root.render(createElement(CodeEditor, { value: CODE, onChange: () => undefined, ...next }));
    });
  };
  await draw(props);
  return {
    draw,
    last: () => seen.props[seen.props.length - 1],
    unmount: async () => {
      await act(async () => {
        root.unmount();
      });
      doc.body.removeChild(container);
    },
  };
}

describe('the page surface', () => {
  it('shows the active line and marks the lines it names', async () => {
    const m = await mount({ page: true, language: 'lua', marks: [STOP] });
    const basic = m.last().basicSetup as Record<string, boolean>;
    expect(basic.highlightActiveLine).toBe(true);
    const view = fakeView();
    await act(async () => {
      (m.last().onCreateEditor as (v: unknown) => void)(view);
    });
    expect(marked(view.state)).toEqual(['  while data.hp < data.maxhp do']);
    // A page with no mark left takes it off.
    await m.draw({ page: true, language: 'lua', marks: [] });
    expect(marked(view.state)).toEqual([]);
    await m.unmount();
  });
});

describe('the lint mark', () => {
  it('leaves the inline editor alone', async () => {
    const m = await mount({ inline: true, language: 'lua', marks: [STOP] });
    const props = m.last();
    expect(props.onCreateEditor).toBeUndefined();
    const basic = props.basicSetup as Record<string, boolean>;
    expect(basic.highlightActiveLine).toBe(false);
    expect(basic.lineNumbers).toBe(false);
    await m.unmount();
  });

  it('leaves the JSON editor, which is not inline, alone too', async () => {
    const m = await mount({ fill: true, marks: [STOP] });
    expect(m.last().onCreateEditor).toBeUndefined();
    await m.unmount();
  });
});
