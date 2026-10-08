import { describe, expect, it } from 'vitest';
import {
  closeSessionQuestion,
  closeWindowQuestion,
  quitQuestion,
  type CloseRow,
} from './closeQuestions';

// The questions Vosh asks before it closes. Close session asks while
// its session is connected and names it as its row does, Close window
// asks while any session is connected, and Quit while two or more are.

const PLAY = 'play.theforsakenlands.com';

function row(id: number, fields: Partial<CloseRow> = {}): CloseRow {
  return { id, name: null, character: null, host: PLAY, port: 1848, connected: true, ...fields };
}

const tolliver = row(1, { character: 'Tolliver' });
const orla = row(2, { character: 'Orla', port: 1825 });

describe('closeSessionQuestion', () => {
  it('asks in board 6 words while the session is connected', () => {
    expect(closeSessionQuestion(2, [tolliver, orla])).toEqual({
      title: "Close Orla's session?",
      body: 'Orla is connected to The Forsaken Lands 1825. Closing this session disconnects Orla and removes the row.',
      confirm: 'Close session',
    });
  });

  it('closes a session that is not connected at once', () => {
    expect(closeSessionQuestion(2, [tolliver, { ...orla, connected: false }])).toBeNull();
    expect(closeSessionQuestion(3, [tolliver, orla])).toBeNull();
  });

  it('reads a session by the name you gave it', () => {
    const named = { ...orla, name: 'Builder' };
    expect(closeSessionQuestion(2, [tolliver, named])).toMatchObject({
      title: "Close Builder's session?",
      body: 'Builder is connected to The Forsaken Lands 1825. Closing this session disconnects Builder and removes the row.',
    });
  });

  it('names the world of a session at the login, before a character', () => {
    const login = row(3, { port: 1825 });
    expect(closeSessionQuestion(3, [tolliver, login])).toEqual({
      title: 'Close this session?',
      body: 'This session is connected to The Forsaken Lands 1825. Closing it ends the connection and removes the row.',
      confirm: 'Close session',
    });
  });
});

describe('closeWindowQuestion', () => {
  it('keeps the words it asked with before sessions', () => {
    expect(closeWindowQuestion([tolliver])).toEqual({
      title: 'Close this window?',
      body: 'You are connected to The Forsaken Lands. Closing this window ends your session and quits Vosh.',
      confirm: 'Close window',
    });
  });

  it('closes at once while no session is connected', () => {
    expect(closeWindowQuestion([{ ...tolliver, connected: false }])).toBeNull();
    expect(
      closeWindowQuestion([
        { ...tolliver, connected: false },
        { ...orla, connected: false },
      ]),
    ).toBeNull();
    expect(closeWindowQuestion([])).toBeNull();
  });

  it('names both connected sessions in board 6 words', () => {
    expect(closeWindowQuestion([tolliver, orla])).toEqual({
      title: 'Close this window?',
      body: 'Two sessions are connected, Tolliver on The Forsaken Lands and Orla on The Forsaken Lands 1825. Closing this window ends both and quits Vosh.',
      confirm: 'Close window',
    });
  });

  it('counts three or more and ends all of them', () => {
    const maren = row(3, { character: 'Maren' });
    expect(closeWindowQuestion([tolliver, maren, orla])?.body).toBe(
      'Three sessions are connected, Tolliver on The Forsaken Lands, Maren on The Forsaken Lands, and Orla on The Forsaken Lands 1825. Closing this window ends all three and quits Vosh.',
    );
  });

  it('leaves a session that is not connected unnamed', () => {
    const maren = row(3, { character: 'Maren', connected: false });
    expect(closeWindowQuestion([tolliver, maren, orla])?.body).toBe(
      'Two sessions are connected, Tolliver on The Forsaken Lands and Orla on The Forsaken Lands 1825. Closing this window ends both and quits Vosh.',
    );
    expect(closeWindowQuestion([{ ...tolliver, connected: false }, orla])?.body).toBe(
      'Orla is connected to The Forsaken Lands 1825. Closing this window disconnects Orla and quits Vosh.',
    );
  });

  it('names a session at the login by its world', () => {
    const login = row(3, { port: 1825 });
    expect(closeWindowQuestion([tolliver, login])?.body).toBe(
      'Two sessions are connected, Tolliver on The Forsaken Lands and one on The Forsaken Lands 1825. Closing this window ends both and quits Vosh.',
    );
    expect(closeWindowQuestion([{ ...tolliver, connected: false }, login])?.body).toBe(
      'A session is connected to The Forsaken Lands 1825. Closing this window ends it and quits Vosh.',
    );
  });

  it('counts past ten in numbers', () => {
    const many = Array.from({ length: 11 }, (_, i) => row(i + 1, { character: 'Tolliver' }));
    expect(closeWindowQuestion(many)?.body).toMatch(
      /^11 sessions are connected, .* Closing this window ends all 11 and quits Vosh\.$/,
    );
  });
});

describe('quitQuestion', () => {
  it('asks in board 6 words while two sessions are connected', () => {
    expect(quitQuestion([tolliver, orla])).toEqual({
      title: 'Quit Vosh?',
      body: 'Two sessions are connected, Tolliver on The Forsaken Lands and Orla on The Forsaken Lands 1825. Quitting ends both.',
      confirm: 'Quit',
    });
  });

  it('quits at once with one session connected, as before sessions', () => {
    expect(quitQuestion([tolliver])).toBeNull();
    expect(quitQuestion([tolliver, { ...orla, connected: false }])).toBeNull();
    expect(quitQuestion([])).toBeNull();
  });

  it('ends all three', () => {
    const maren = row(3, { character: 'Maren' });
    expect(quitQuestion([tolliver, maren, orla])?.body).toBe(
      'Three sessions are connected, Tolliver on The Forsaken Lands, Maren on The Forsaken Lands, and Orla on The Forsaken Lands 1825. Quitting ends all three.',
    );
  });
});
