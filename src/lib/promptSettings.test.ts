import { describe, expect, it } from 'vitest';
import {
  codesMeta,
  drawDescription,
  gameBlock,
  gameCodesOf,
  gameDescription,
  lastReadLine,
  previewHeight,
  previewMeta,
  previewOptions,
  promptWorld,
  settingsMatchLine,
  type GameCodes,
} from './promptSettings';
import type {
  PromptCapture,
  PromptCaptureCheck,
  PromptCompileReport,
  PromptLastSeen,
} from './session';

const none: PromptCapture = { kind: 'none' };
const codes: PromptCapture = {
  kind: 'aabahran',
  prompt: '%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c',
  fprompt: '',
  follow_game: true,
  seen_at: '2026-09-29T12:58:02-05:00',
  source: 'gmcp',
};
const migrated: PromptCapture = {
  kind: 'regex',
  lines: ['\\[(?<hp>\\d+)/(?<maxhp>\\d+)hp'],
  settle: false,
  source: 'migrated',
};
const pointed: PromptCapture = {
  kind: 'regex',
  lines: ['^<(\\d+)/(\\d+)hp (\\d+)/(\\d+)m (\\d+)/(\\d+)mv>$'],
  settle: true,
  source: 'typed',
};

const login: GameCodes = {
  prompt: '%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c',
  fprompt: '',
  enabled: true,
  at: new Date(2026, 8, 29, 12, 58).getTime(),
  atLogin: true,
};

function check(matched: number, total: number, text = ''): PromptCaptureCheck {
  return { matched, total, fight_matched: 0, false_matches: 0, text, reads: [] };
}

function report(over: Partial<PromptCompileReport> = {}): PromptCompileReport {
  return {
    ok: true,
    error: null,
    prompt: '<%h%m %vmv> ',
    fprompt: '',
    shapes: [],
    vars: [],
    codes: [],
    warnings: [],
    presets: [],
    names: {},
    numbers: [],
    legend: [],
    shows: null,
    fix_note: null,
    fixes: [],
    gmcp_names: [],
    ...over,
  };
}

describe('the game prompt block in Settings', () => {
  it('shows the codes the game sent as text on the new build (D25)', () => {
    for (const capture of [none, codes, migrated, pointed]) {
      expect(gameBlock({ forsaken: true, gameSent: true, capture })).toBe('codes');
    }
  });

  it('holds fields for your codes on The Forsaken Lands without Char.Prompt', () => {
    expect(gameBlock({ forsaken: true, gameSent: false, capture: codes })).toBe('fields');
    expect(gameBlock({ forsaken: true, gameSent: false, capture: none })).toBe('fields');
    // The pattern your old capture trigger left counts as no codes yet.
    expect(gameBlock({ forsaken: true, gameSent: false, capture: migrated })).toBe('fields');
  });

  it('shows the line you pointed at for a pattern, on any other game', () => {
    expect(gameBlock({ forsaken: false, gameSent: false, capture: pointed })).toBe('line');
    expect(gameBlock({ forsaken: true, gameSent: false, capture: pointed })).toBe('line');
    expect(gameBlock({ forsaken: false, gameSent: false, capture: migrated })).toBe('line');
    expect(gameBlock({ forsaken: false, gameSent: false, capture: none })).toBe('point');
  });

  it('reads the codes from the store, or from the backend when it holds none', () => {
    const seen: PromptLastSeen = {
      prompt: '<%h%m %vmv> ',
      fprompt: null,
      enabled: true,
      at: '2026-09-29T12:58:00',
      at_login: true,
      source: 'gmcp',
      character: 'Tester',
    };
    const stored = { ...login, receivedAt: login.at };
    expect(gameCodesOf(stored, seen)).toEqual(login);
    expect(gameCodesOf(null, seen)).toEqual({
      prompt: '<%h%m %vmv> ',
      fprompt: '',
      enabled: true,
      at: new Date(2026, 8, 29, 12, 58).getTime(),
      atLogin: true,
    });
    // Only Char.Prompt counts. What you typed in an earlier session does
    // not.
    expect(gameCodesOf(null, { ...seen, source: 'session' })).toBeNull();
    expect(gameCodesOf(null, { ...seen, prompt: null })).toBeNull();
    expect(gameCodesOf(null, null)).toBeNull();
  });

  it('names the game in its description', () => {
    expect(gameDescription('The Forsaken Lands')).toBe(
      'Your prompt setting in The Forsaken Lands. Vosh reads its codes.',
    );
    expect(gameDescription(null)).toBe('Your prompt setting in the game. Vosh reads its codes.');
  });

  it('calls the world by its name under the Forsaken Lands rules', () => {
    expect(promptWorld({ forsaken: true, host: 'play.theforsakenlands.com' })).toBe(
      'The Forsaken Lands',
    );
    expect(promptWorld({ forsaken: true, host: '127.0.0.1' })).toBe('The Forsaken Lands');
    expect(promptWorld({ forsaken: false, host: 'mud.example.net' })).toBe('mud.example.net');
    expect(promptWorld({ forsaken: false, host: '' })).toBeNull();
  });
});

describe('the meta under the game prompt', () => {
  const now = new Date(2026, 8, 29, 13, 30);

  it('says the game sent it, then the match count (P12)', () => {
    expect(
      codesMeta({
        block: 'codes',
        game: login,
        seen: null,
        capture: codes,
        check: check(14, 14),
        report: report(),
        promptsOff: false,
        now,
      }),
    ).toEqual({
      tone: 'normal',
      text: 'The game sent it when you logged in. Matches your last 14 prompts.',
      fixes: [],
    });
    expect(
      codesMeta({
        block: 'codes',
        game: { ...login, atLogin: false },
        seen: null,
        capture: codes,
        check: check(1, 1),
        report: report(),
        promptsOff: false,
        now,
      }).text,
    ).toBe('The game sent it at 12:58. Matches your last prompt.');
  });

  it('leaves the count out with no capture yet (P13)', () => {
    expect(
      codesMeta({
        block: 'codes',
        game: { ...login, prompt: '<%h%m %vmv> ' },
        seen: null,
        capture: none,
        check: null,
        report: report(),
        promptsOff: false,
        now,
      }).text,
    ).toBe('The game sent it when you logged in.');
  });

  it('puts the warning and its command in place of the meta while codes run together', () => {
    const together = report({
      warnings: [
        {
          kind: 'run_together',
          which: 'prompt',
          span: [1, 5],
          message:
            'Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game.',
        },
      ],
      fixes: ['prompt <%h %m %vmv>'],
    });
    expect(
      codesMeta({
        block: 'codes',
        game: login,
        seen: null,
        capture: codes,
        check: check(3, 3),
        report: together,
        promptsOff: false,
        now,
      }),
    ).toEqual({
      tone: 'warn',
      text: 'Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game.',
      fixes: ['prompt <%h %m %vmv>'],
    });
  });

  it('says why no prompt arrives while prompts are off (P14)', () => {
    expect(
      codesMeta({
        block: 'codes',
        game: login,
        seen: null,
        capture: codes,
        check: check(14, 14),
        report: report(),
        promptsOff: true,
        now,
      }),
    ).toEqual({
      tone: 'warn',
      text: 'You turned prompts off in the game. Type prompt in the game to turn them back on.',
      fixes: [],
    });
  });

  it('says where Vosh saw your codes without Char.Prompt', () => {
    const seen: PromptLastSeen = {
      prompt: '%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c',
      fprompt: '',
      enabled: true,
      at: '2026-09-29T05:04:00',
      at_login: false,
      source: 'session',
      character: 'Tester',
    };
    const base = {
      block: 'fields' as const,
      game: null,
      seen,
      capture: { ...codes, source: 'session' as const },
      check: check(14, 14),
      report: report(),
      promptsOff: false,
      now,
    };
    expect(codesMeta(base).text).toBe(
      'Vosh saw it when you typed prompt at 5:04. Matches your last 14 prompts.',
    );
    // A trailing space the game adds does not make the codes differ.
    expect(codesMeta({ ...base, seen: { ...seen, prompt: `${seen.prompt} ` } }).text).toBe(
      'Vosh saw it when you typed prompt at 5:04. Matches your last 14 prompts.',
    );
    // Codes you typed since say no source.
    expect(
      codesMeta({ ...base, capture: { ...codes, prompt: '%h %m %v ', source: 'typed' } }).text,
    ).toBe('Matches your last 14 prompts.');
    // With no capture the meta asks for your setting.
    expect(codesMeta({ ...base, seen: null, capture: none, check: null }).text).toBe(
      'Type prompt in the game and Vosh reads the answer.',
    );
  });

  it('says a compile error in warn', () => {
    expect(
      codesMeta({
        block: 'fields',
        game: null,
        seen: null,
        capture: codes,
        check: null,
        report: report({
          ok: false,
          error: {
            message: 'A color code runs into %h. Put a space between them in the game.',
            which: 'prompt',
            span: null,
          },
        }),
        promptsOff: false,
        now,
      }),
    ).toEqual({
      tone: 'warn',
      text: 'A color code runs into %h. Put a space between them in the game.',
      fixes: [],
    });
  });
});

describe('the match count in Settings', () => {
  it('counts your prompts, or says why there are none', () => {
    expect(settingsMatchLine(check(14, 14))).toBe('Matches your last 14 prompts.');
    expect(settingsMatchLine(check(1, 3))).toBe('Matches your last prompt.');
    expect(
      settingsMatchLine(check(0, 0, 'Vosh has not seen your prompt since you connected.')),
    ).toBeNull();
    expect(
      settingsMatchLine(
        check(0, 3, 'Does not match any of the 3 lines before your last commands.'),
      ),
    ).toBe('Does not match any of the 3 lines before your last commands.');
    expect(settingsMatchLine(null)).toBeNull();
  });
});

describe('the line another game prints', () => {
  it('says when Vosh last read it', () => {
    expect(lastReadLine('2026-09-29T08:42:10-05:00')).toMatch(/^Last read at \d{1,2}:\d{2}$/);
    expect(lastReadLine(null)).toBeNull();
    expect(lastReadLine('not a time')).toBeNull();
  });
});

describe('the Draw your own prompt row', () => {
  it('says what drawing does, and what it waits on', () => {
    expect(
      drawDescription({ capture: true, draw: true, gameSent: true, world: 'The Forsaken Lands' }),
    ).toBe('It takes the place of the prompt The Forsaken Lands sends.');
    expect(
      drawDescription({ capture: true, draw: true, gameSent: false, world: 'mud.example.net' }),
    ).toBe('It takes the place of the prompt mud.example.net sends.');
    expect(drawDescription({ capture: true, draw: true, gameSent: false, world: null })).toBe(
      'It takes the place of the prompt the game sends.',
    );
    expect(
      drawDescription({ capture: true, draw: false, gameSent: true, world: 'The Forsaken Lands' }),
    ).toBe("The game's prompt shows as it arrives. Your design stays saved.");
    expect(
      drawDescription({ capture: false, draw: false, gameSent: true, world: 'The Forsaken Lands' }),
    ).toBe('Customize your prompt first.');
    expect(
      drawDescription({ capture: false, draw: true, gameSent: false, world: 'The Forsaken Lands' }),
    ).toBe("Tell Vosh your game's prompt first.");
  });
});

describe('the preview block', () => {
  it('grows 17.5 for each line past the first', () => {
    expect(previewHeight(1)).toBe(28);
    expect(previewHeight(2)).toBe(45.5);
    expect(previewHeight(3)).toBe(63);
    expect(previewHeight(0)).toBe(28);
  });

  it('offers Lament only under the Forsaken Lands rules', () => {
    expect(previewOptions(true).map((o) => o.label)).toEqual([
      'Now',
      'Low health',
      'Fight',
      'Lament',
    ]);
    expect(previewOptions(false).map((o) => o.value)).toEqual(['now', 'low_health', 'fight']);
  });

  it('says where to change it, or that it draws samples offline', () => {
    expect(previewMeta(true)).toBe('Right click your prompt in the terminal to change it there.');
    expect(previewMeta(false)).toBe('Sample values until you connect.');
  });
});
