import type { OptionPickerGroup, OptionPickerOption } from "./OptionPickerItem";

export function matches(option: OptionPickerOption, needle: string): boolean {
  return `${option.label} ${option.detail ?? ""}`
    .toLocaleLowerCase()
    .includes(needle);
}

/**
 * A group stays whole when its heading matches, and otherwise keeps only the
 * options that match; a group with neither goes. Unfiltered, a long group is
 * cut to `limit` and says how many more the filter can reach.
 */
export function filterGroups(
  groups: readonly OptionPickerGroup[],
  needle: string,
  limit: number,
  moreLabel?: (hidden: number) => string,
): OptionPickerGroup[] {
  if (!needle) {
    return groups.map((group) =>
      group.options.length > limit && moreLabel
        ? {
            ...group,
            options: group.options.slice(0, limit),
            note: moreLabel(group.options.length - limit),
          }
        : group,
    );
  }
  return groups.flatMap((group) => {
    if (matches(group.header, needle)) return [group];
    const options = group.options.filter((option) => matches(option, needle));
    return options.length > 0 ? [{ ...group, options, note: null }] : [];
  });
}
