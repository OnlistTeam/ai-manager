/** Settings sections another page can link straight to. */
export const SETTINGS_SECTIONS = ["privacy"] as const;

export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

const SECTION_ATTRIBUTE = "data-settings-section";

/** Props that mark an element as the target of a section link. */
export function settingsSectionProps(section: SettingsSection) {
  return { [SECTION_ATTRIBUTE]: section };
}

/** Scrolls a section into view and moves keyboard focus to its heading. */
export function focusSettingsSection(section: SettingsSection): void {
  const target = document.querySelector<HTMLElement>(
    `[${SECTION_ATTRIBUTE}="${section}"]`,
  );
  target?.scrollIntoView?.({ block: "start", behavior: "smooth" });
  target?.querySelector<HTMLElement>("h2")?.focus({ preventScroll: true });
}
