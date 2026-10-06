import { describe, expect, it } from 'vitest';
import { closeSessionQuestion, closeWindowQuestion, type CloseRow } from './closeQuestions';

// Q13 and board 6 of the Sessions review. Close session asks while its
// session is connected and names it as its row does, and Close window
// asks while any session is connected.

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
    expect(closeWindowQuestion([])).toBeNull();
  });
});
