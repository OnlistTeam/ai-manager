/** Settings sections another page can link straight to. */
export const SETTINGS_SECTIONS = ["privacy"] as const;

export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

const SECTION_ATTRIBUTE = "data-settings-section";

/** Props that mark an element as the target of a section link. */
export function settingsSectionProps(section: SettingsSection) {
  return { [SECTION_ATTRIBUTE]: section };
}

/**
 * Moves keyboard focus to a section's heading and scrolls it into view. The
 * scroll waits for the page's entrance animation: measured while the page is
 * still sliding in, the section would land under the title bar.
 */
export async function focusSettingsSection(
  section: SettingsSection,
): Promise<void> {
  const target = document.querySelector<HTMLElement>(
    `[${SECTION_ATTRIBUTE}="${section}"]`,
  );
  if (!target) return;
  target.querySelector<HTMLElement>("h2")?.focus({ preventScroll: true });
  const layer = target.closest<HTMLElement>("[data-app-route-content]");
  const entering = layer?.getAnimations?.() ?? [];
  await Promise.allSettled(entering.map((animation) => animation.finished));
  target.scrollIntoView?.({ block: "start", behavior: "smooth" });
}
