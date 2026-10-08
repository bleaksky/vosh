import {
  DEFAULT_ECHO_MARK_OPTIONS,
  echoMarkOptionsOf,
  getUiConfig,
  subscribeInputEchoMarkChanged,
  type EchoMarkOptions,
} from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The mark the echo of each command you send starts with, its color and
// Dim sent commands, from Settings, Input. The command line builds your
// echo from it and the terminal leaves the same mark out after a prompt
// that ends in >.

function same(a: EchoMarkOptions, b: EchoMarkOptions): boolean {
  return a.mark === b.mark && a.text === b.text && a.color === b.color && a.dim === b.dim;
}

const store = createConfigStore<EchoMarkOptions>({
  initial: DEFAULT_ECHO_MARK_OPTIONS,
  read: () => getUiConfig().then(echoMarkOptionsOf),
  follow: subscribeInputEchoMarkChanged,
  same,
});

export const startEchoMarkStore = store.start;
export const getEchoMarkOptions = store.get;
export const subscribeEchoMarkOptions = store.subscribe;
export const useEchoMarkOptions = store.use;
