import type { SettingsPageProps } from '../pageTypes';
import { Button, Row, Section } from '../ui';

// Layout has no approved board yet, and nothing the old window showed
// belongs here now: the vitals layouts, dock zones, chip style, and
// moons position went with the One Window frame, and the panel layout
// and tracked affects moved to Characters. This block says where to
// look until the Layout board lands.
export function LayoutGroup({ navigate }: SettingsPageProps) {
  return (
    <Section id="panel" title="Panel">
      <div data-interim="">
        <Row
          label="Panel layout and tracked affects"
          description="Arrange panes from the more button on each pane. Characters keeps each character's panes and tracked affects."
        >
          <Button onClick={() => navigate({ group: 'characters', anchor: 'layout' })}>
            Open Characters
          </Button>
        </Row>
      </div>
    </Section>
  );
}
