import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { PluginInstallCheck } from '../../ipc/scripts';
import { InstallDialog } from './InstallDialog';

// Install's one question as markup.

const none = () => undefined;

// A sample plugin with a pane, weather_pane.
const WEATHER: PluginInstallCheck = {
  name: 'weather_pane',
  version: '0.2.0',
  author: 'Tolliver',
  existing: null,
};

const WARNING =
  'A plugin can send commands to the game and read everything the game sends. Install plugins only from people you trust.';

function draw(check: PluginInstallCheck, busy = false) {
  const html = renderToStaticMarkup(
    <InstallDialog check={check} busy={busy} onInstall={none} onCancel={none} />,
  );
  return {
    title: /class="ov-confirm-title">([^<]*)</.exec(html)?.[1],
    body: /class="ov-confirm-body">([^<]*)</.exec(html)?.[1],
    install: /<button type="button"( disabled="")? class="btn is-primary">Install</.exec(html),
  };
}

describe('the Install question', () => {
  it('names the plugin, its version and its author over the warning', () => {
    const { title, body, install } = draw(WEATHER);
    expect(title).toBe('Install weather_pane?');
    expect(body).toBe(`weather_pane 0.2.0 by Tolliver. ${WARNING}`);
    expect(install?.[1]).toBeUndefined();
  });

  it('says which plugin goes and that the install turns it off everywhere', () => {
    const { body } = draw({ ...WEATHER, existing: { version: '0.1.0', on_in: ['Healer'] } });
    expect(body).toBe(
      `weather_pane 0.2.0 by Tolliver. ${WARNING} You have weather_pane 0.1.0, on in Healer. Installing replaces it and turns it off in every profile.`,
    );
  });

  it('names every profile that turns the old plugin on', () => {
    const { body } = draw({
      ...WEATHER,
      existing: { version: '0.1.0', on_in: ['Default', 'Healer'] },
    });
    expect(body).toContain('You have weather_pane 0.1.0, on in Default and Healer.');
  });

  it('says only that it replaces a plugin no profile turns on', () => {
    const { body } = draw({ ...WEATHER, existing: { version: '0.1.0', on_in: [] } });
    expect(body).toBe(
      `weather_pane 0.2.0 by Tolliver. ${WARNING} You have weather_pane 0.1.0. Installing replaces it.`,
    );
  });

  it('leaves out a version or an author the manifest does not give', () => {
    expect(draw({ ...WEATHER, version: '', author: '' }).body).toBe(`weather_pane. ${WARNING}`);
    expect(draw({ ...WEATHER, author: '' }).body).toBe(`weather_pane 0.2.0. ${WARNING}`);
  });

  it('holds Install off while the install runs', () => {
    expect(draw(WEATHER, true).install?.[1]).toBe(' disabled=""');
  });
});
