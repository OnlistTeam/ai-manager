import { useCallback, useState } from "react";
import type { ToolId } from "@/entities/tool";
import type { ModelMenuItem } from "./homeModelMenu";

const STORAGE_KEY = "aimanager.home.modelFavorites";

function readStored(): string[] {
  try {
    const stored: unknown = JSON.parse(
      window.localStorage.getItem(STORAGE_KEY) ?? "[]",
    );
    return Array.isArray(stored)
      ? stored.filter((key): key is string => typeof key === "string")
      : [];
  } catch {
    // Unreadable or unavailable storage starts with no favourites.
    return [];
  }
}

export interface ModelFavorites {
  isFavorite: (item: ModelMenuItem) => boolean;
  toggle: (item: ModelMenuItem) => void;
}

/**
 * The models starred in a row's model picker, per tool and endpoint, since a
 * model name means something only to the endpoint it was listed under. A
 * pure view preference, remembered on this device only.
 */
export function useModelFavorites(tool: ToolId): ModelFavorites {
  const [keys, setKeys] = useState(readStored);
  const keyOf = useCallback(
    (item: ModelMenuItem) =>
      item.model === null
        ? null
        : [tool, item.endpoint.key, item.model].join("\u001f"),
    [tool],
  );

  const isFavorite = useCallback(
    (item: ModelMenuItem) => {
      const key = keyOf(item);
      return key !== null && keys.includes(key);
    },
    [keyOf, keys],
  );

  const toggle = useCallback(
    (item: ModelMenuItem) => {
      const key = keyOf(item);
      if (key === null) return;
      // Read again so another row's stars, saved since, are kept.
      const stored = readStored();
      const next = stored.includes(key)
        ? stored.filter((entry) => entry !== key)
        : [...stored, key];
      setKeys(next);
      try {
        window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        // Not remembered this time; the star still shows for this view.
      }
    },
    [keyOf],
  );

  return { isFavorite, toggle };
}
