import { Fragment, useRef, useState, type ReactNode } from 'react';
import { openVitalsTextCard } from '../ipc/prompt';
import { openSettingsTab } from '../lib/settingsLink';
import { formatSettingsTarget } from '../lib/settingsNav';
import { useVitalsOptions } from '../stores/config/vitalsOptionsStore';
import { submenuAt } from '../ui/menuPlacement';
import { MenuItem, MenuSeparator, MenuSurface, type MenuCloseReason } from '../ui/MenuSurface';
import { CheckIcon, ChevronRightIcon } from '../ui/icons';
import { openPaneSubmenu, type MenuChoice, type PaneSubmenuState } from './affects/affectsDisplay';
import { returnToCommandLine } from './paneActions';
import {
  pickVitalsStyle,
  pickVitalsValues,
  vitalsStyleFamilies,
  vitalsValuesChoices,
} from './vitalsPicks';

// The menu a right click on your vitals opens, on the panel footer or
// on the status line's vitals. It opens at the pointer, as the terminal's menu does, with
// Style and Values, each a submenu with a check on your pick, Style's
// with a line between families of styles, then
// Customize vitals…, which opens Settings there. Under Text it adds Edit
// your text…, which opens the card for your text, and Values goes quiet, since your text writes its own
// values. Colors stay in Customize vitals. A pick saves alone for the
// profile, then tells every window, so the footer, the status line and
// Settings follow at once. The footer and the status line take no
// focus, so the keyboard reaches every choice in Settings.

type VitalsSubmenu = 'style' | 'values';

const MENU_ID = 'vitals-menu';
const subId = (which: VitalsSubmenu) => `${MENU_ID}-${which}`;

/** A submenu's choices, a check on the pick. A pick closes the menu. */
export function VitalsChoiceItems<T extends string>({
  choices,
  pick,
  done,
}: {
  choices: MenuChoice<T>[];
  pick: (value: T) => Promise<void>;
  done: () => void;
}) {
  return (
    <>
      {choices.map((choice) => (
        <MenuItem
          key={choice.value}
          onSelect={() => {
            done();
            void pick(choice.value).catch(() => undefined);
          }}
          trailing={choice.checked ? <CheckIcon className="pane-menu-check" /> : null}
        >
          {choice.label}
        </MenuItem>
      ))}
    </>
  );
}

const openSettingsAt = () =>
  openSettingsTab(formatSettingsTarget({ group: 'layout', section: 'customize-vitals' }));

interface Props {
  /** The pointer, where the menu opens. */
  x: number;
  y: number;
  onClose: () => void;
}

export function VitalsMenu({ x, y, onClose }: Props) {
  const options = useVitalsOptions();
  const [subOpen, setSubOpen] = useState<PaneSubmenuState<VitalsSubmenu> | null>(null);
  const rowRefs = useRef<Partial<Record<VitalsSubmenu, HTMLButtonElement | null>>>({});
  const text = options.style === 'text';

  // Closing hands the caret back to the command line, unless you
  // clicked somewhere else on purpose.
  const close = (reason: MenuCloseReason | 'select') => {
    onClose();
    if (reason !== 'outside') returnToCommandLine();
  };
  const done = () => close('select');
  const closeSub = () => setSubOpen(null);

  const submenus: Record<VitalsSubmenu, { label: string; items: () => ReactNode }> = {
    style: {
      label: 'Style',
      // A line sets each family of styles apart.
      items: () =>
        vitalsStyleFamilies(options).map((family, i) => (
          <Fragment key={family[0]?.value}>
            {i > 0 && <MenuSeparator />}
            <VitalsChoiceItems choices={family} pick={pickVitalsStyle} done={done} />
          </Fragment>
        )),
    },
    values: {
      label: 'Values',
      items: () => (
        <VitalsChoiceItems
          choices={vitalsValuesChoices(options)}
          pick={pickVitalsValues}
          done={done}
        />
      ),
    },
  };

  let sub: ReactNode = null;
  const row = subOpen ? rowRefs.current[subOpen.which] : null;
  if (subOpen && row) {
    const which = subOpen.which;
    const r = row.getBoundingClientRect();
    const menu = row.closest('menu')?.getBoundingClientRect() ?? r;
    sub = (
      <MenuSurface
        key={which}
        id={subId(which)}
        label={submenus[which].label}
        nested
        autoFocus={subOpen.focus}
        className="pane-menu-sub"
        at={submenuAt(r, menu)}
        onClose={() => {
          // Escape or ArrowLeft: back to the row that opened it.
          setSubOpen(null);
          rowRefs.current[which]?.focus();
        }}
      >
        {submenus[which].items()}
      </MenuSurface>
    );
  }

  const submenuRow = (which: VitalsSubmenu, disabled: boolean) => (
    <MenuItem
      itemRef={(el) => {
        rowRefs.current[which] = el;
      }}
      disabled={disabled}
      onFocus={() => setSubOpen((prev) => (prev && prev.which !== which ? null : prev))}
      submenu={{
        open: subOpen?.which === which,
        controls: subId(which),
        onOpen: (focus) => setSubOpen((prev) => openPaneSubmenu(prev, which, focus)),
      }}
      trailing={<ChevronRightIcon className="pane-menu-chevron" />}
    >
      {submenus[which].label}
    </MenuItem>
  );

  return (
    <>
      <MenuSurface id={MENU_ID} label="Vitals" at={{ x, y }} onClose={close}>
        {submenuRow('style', false)}
        {submenuRow('values', text)}
        <MenuSeparator />
        {text && (
          <MenuItem
            onHover={closeSub}
            onFocus={closeSub}
            onSelect={() => {
              onClose();
              void openVitalsTextCard().catch(() => undefined);
            }}
          >
            Edit your text…
          </MenuItem>
        )}
        <MenuItem
          onHover={closeSub}
          onFocus={closeSub}
          onSelect={() => {
            done();
            openSettingsAt();
          }}
        >
          Customize vitals…
        </MenuItem>
      </MenuSurface>
      {sub}
    </>
  );
}
