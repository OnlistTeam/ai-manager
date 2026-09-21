import { writeProviderPresetCatalog } from "./provider-preset-catalog.mjs";

const result = await writeProviderPresetCatalog();
console.log(
  `${result.changed ? "Updated" : "Verified"} provider preset catalog (${result.count} entries).`,
);
