// The window's keys and menus reach the snoop split through these. Cmd
// J from the command line puts the caret in the snoop in front, and
// pressed again in a snoop it steps to the next tab. Cmd F and Copy act
// on the snoop while its terminal holds the caret. The split hears each
// request on the window, as the session popover hears its own.

/** What the window asks of the snoop split: put the caret in the tab in
 *  front, step to the next tab and put it there, open Find on the tab in
 *  front, or copy what you selected in it. */
export type SnoopRequest = 'enter' | 'next' | 'find' | 'copy';

/** The window event a request goes out on. */
export const SNOOP_REQUEST_EVENT = 'vosh:snoop';

/** Ask the snoop split for `request`. */
export function requestSnoop(request: SnoopRequest): void {
  window.dispatchEvent(new CustomEvent<SnoopRequest>(SNOOP_REQUEST_EVENT, { detail: request }));
}

/** Whether a snoop terminal holds the caret. */
export function snoopHasCaret(doc: Pick<Document, 'activeElement'> = document): boolean {
  return doc.activeElement?.closest('.snoop-term') != null;
}

/** Cmd J: into the snoop from anywhere else, and to the next tab from
 *  inside one. */
export function goToSnoop(): void {
  requestSnoop(snoopHasCaret() ? 'next' : 'enter');
}
