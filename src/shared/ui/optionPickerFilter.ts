import type { OptionPickerOption } from "./OptionPickerItem";

export function matches(option: OptionPickerOption, needle: string): boolean {
  return `${option.label} ${option.detail ?? ""}`
    .toLocaleLowerCase()
    .includes(needle);
}
