import {
  keepPreviousData,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type DiscoverMcpList, type DiscoverSkillList } from "@/native";
import { discoverKeys } from "./keys";

/**
 * The query stays on the last answer while the next one loads, so the grid
 * dims instead of emptying on every keystroke. No retry: a source that is
 * down answers with featured or cached items and one quiet error line.
 */
export function useDiscoverMcp(
  query: string,
  enabled: boolean,
): UseQueryResult<DiscoverMcpList, Error> {
  return useQuery({
    queryKey: discoverKeys.list("mcp", query),
    queryFn: () => native.discover.mcpList(query),
    enabled,
    placeholderData: keepPreviousData,
    retry: false,
    ...sessionCacheOptions,
  });
}

export function useDiscoverSkills(
  query: string,
  enabled: boolean,
): UseQueryResult<DiscoverSkillList, Error> {
  return useQuery({
    queryKey: discoverKeys.list("skill", query),
    queryFn: () => native.discover.skillList(query),
    enabled,
    placeholderData: keepPreviousData,
    retry: false,
    ...sessionCacheOptions,
  });
}

/** What the shown Skills say of themselves, fetched once per set of cards. */
export function useDiscoverSkillDescriptions(
  ids: readonly string[],
): UseQueryResult<Record<string, string>, Error> {
  return useQuery({
    queryKey: discoverKeys.descriptions(ids),
    queryFn: () => native.discover.skillDescriptions(ids),
    enabled: ids.length > 0,
    retry: false,
    ...sessionCacheOptions,
  });
}

/** A card's picture; a failure leaves the initial on its tile. */
export function useDiscoverIcon(
  url: string | null,
): UseQueryResult<string, Error> {
  return useQuery({
    queryKey: discoverKeys.icon(url ?? ""),
    queryFn: () => native.discover.icon(url ?? ""),
    enabled: url !== null && url !== "",
    retry: false,
    ...sessionCacheOptions,
  });
}
