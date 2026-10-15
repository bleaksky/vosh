import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import panelCss from '../../styles/panel.css?raw';
import { parseStats, type Stats } from '../../stores/gmcp/statsStore';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { StatsBody } from './StatsPane';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const STATS = parseStats(aabahranPacket('char-stats.gmcp').data) as Stats;
const draw = (style: 'sheet' | 'rows' | 'ledger', stats: Stats = STATS) =>
  renderToStaticMarkup(<StatsBody stats={stats} style={style} />);

describe('the Stats pane', () => {
  for (const style of ['sheet', 'rows', 'ledger'] as const) {
    describe(style, () => {
      it('shows every number on the score sheet', () => {
        const html = draw(style);
        for (const text of ['22', '31', '27', '−310', '−295', '−12', '+2']) {
          expect(html, text).toContain(text);
        }
        expect(html).toContain('aria-label="Luck 4 of 6"');
        expect(html).toContain('Armor');
        expect(html).toContain('Saves');
      });

      it('colors the stats, the armor and the saves as the score sheet does', () => {
        const html = draw(style);
        expect(html).toMatch(/is-up">22</);
        expect(html).toMatch(/is-down">15</);
        expect(html).toMatch(/is-danger">\+2</);
        expect(html).toMatch(/is-quiet">0</);
      });

      it('marks the points past a max the way the score sheet does, and says so aloud', () => {
        const html = draw(style);
        expect(html).toContain('aria-hidden="true">^1<');
        expect(html).toContain(', 1 past the max');
      });

      it('leaves out a part the game did not send', () => {
        const html = draw(style, parseStats({ str: { cur: 18, perm: 18, max: 20 } }) as Stats);
        expect(html).toContain('18');
        expect(html).not.toContain('Armor');
        expect(html).not.toContain('Saves');
        expect(html).not.toContain('Luck');
      });
    });
  }

  it('writes a stat the game way on the score sheet, trained then current in brackets', () => {
    expect(draw('sheet')).toMatch(
      />20<span class="pane-stats-sheet-paren"> \(<span class="pane-stats-sheet-cur is-up">22</,
    );
  });

  it('lays out armor in two columns or one, so no kind sits alone', () => {
    const at = panelCss.indexOf('.pane-stats-ledger.is-armor {');
    expect(panelCss.slice(at, panelCss.indexOf('}', at))).toContain('(100% - 12px) / 2');
  });
});
