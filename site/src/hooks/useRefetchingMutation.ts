import type { ApolloCache, ApolloClient, DocumentNode, OperationVariables } from '@apollo/client';
import { useApolloClient, useMutation } from '@apollo/client/react';
import { useCallback, useEffect, useRef } from 'react';

/**
 * The list and summary queries every write can change (spec refetch policy).
 * Entity writes add `GetEntity`.
 */
export const REFETCH_AFTER_WRITE = [
  'GetLocations',
  'GetSummary',
  'GetRootItems',
  'GetEntityTypes',
  'GetTags',
] as const;

/**
 * The `ROOT_QUERY` field each list query caches its answer under. A write
 * evicts these for the queries it names that are not mounted, so the next
 * mount goes to the network instead of answering from a stale cache.
 */
const ROOT_FIELDS: Readonly<Record<string, string>> = {
  GetSummary: 'summary',
  GetRootItems: 'rootItems',
  GetLocations: 'locations',
  GetEntityTypes: 'entityTypes',
  GetTags: 'tags',
};

type EntityIds = ReadonlyArray<string | null | undefined>;

export interface RefetchingMutationOptions<TData, TVariables> {
  /** Operation names to refetch once the mutation succeeds; only active ones are refetched. */
  refetch: readonly string[];
  /** Ids of `Entity` cache entries the result makes stale (a parent whose children changed, a deleted entity). */
  evict?: (data: TData, variables: TVariables) => EntityIds;
}

/**
 * Drops each `Entity:<id>` entry, the root field of every named query that is
 * not active, and every cached `search` answer (whatever its arguments), then
 * garbage-collects what only they referenced.
 */
function evictStale(cache: ApolloCache, ids: EntityIds, inactive: readonly string[]): void {
  for (const id of ids) {
    if (!id) continue;
    cache.evict({ id: cache.identify({ __typename: 'Entity', id }) });
  }
  for (const name of inactive) {
    const fieldName = ROOT_FIELDS[name];
    if (fieldName) cache.evict({ id: 'ROOT_QUERY', fieldName });
  }
  cache.evict({ id: 'ROOT_QUERY', fieldName: 'search' });
  cache.gc();
}

/** The names of the queries currently mounted; resolved at completion, not render. */
const activeQueryNames = (client: ApolloClient): Set<string> =>
  new Set([...client.getObservableQueries('active')].map((query) => query.queryName ?? ''));

/**
 * The same refetch policy for a write that did not go through Apollo (the
 * `fetch`-based photo upload): evicts the `Entity` entries in `ids`, the root
 * field of each query in `refetch` that is not active, and every cached
 * search, then refetches the ones in `refetch` that are active and resolves
 * once they land. Evicting first matters for the same reason as below.
 */
export async function refetchAfterWrite(
  client: ApolloClient,
  refetch: readonly string[],
  ids: EntityIds,
): Promise<void> {
  const active = activeQueryNames(client);
  evictStale(
    client.cache,
    ids,
    refetch.filter((name) => !active.has(name)),
  );
  const include = refetch.filter((name) => active.has(name));
  if (include.length > 0) await client.refetchQueries({ include });
}

/**
 * `useMutation` plus this app's refetch policy. On success it evicts the
 * `Entity` entries named by `evict` (and by the call's `alsoEvict`, for ids
 * only the caller knows, such as a moved entity's old parent), evicts the
 * root field of each named query that is not active (and every cached
 * search), then refetches the named queries that are active, and resolves
 * only once those refetches land (`awaitRefetchQueries`).
 *
 * Eviction runs in the mutation's cache update, before the refetches, so a
 * refetched list (the sidebar's `GetLocations`) rewrites any entry it shares
 * with an evicted entity instead of being left with a dangling reference (which
 * Apollo would answer with a second fetch); an evicted entity still shown by an
 * active `GetEntity` refetches itself.
 *
 * Returns `[run, { loading, error }]`; `run(variables, alsoEvict?)` resolves to
 * the mutation's data and rejects on failure.
 */
export function useRefetchingMutation<TData, TVariables extends OperationVariables>(
  mutation: DocumentNode,
  { refetch, evict }: RefetchingMutationOptions<TData, TVariables>,
) {
  const client = useApolloClient();
  // Read at completion through a ref, so an inline `evict` doesn't give `run` a new identity per render.
  const evictRef = useRef(evict);
  const refetchRef = useRef(refetch);
  useEffect(() => {
    evictRef.current = evict;
    refetchRef.current = refetch;
  }, [evict, refetch]);
  // Resolved at completion, not render: naming an inactive query makes Apollo warn.
  const activeNames = useCallback(() => activeQueryNames(client), [client]);
  const [mutate, { loading, error }] = useMutation<TData, TVariables>(mutation, {
    awaitRefetchQueries: true,
    refetchQueries: () => {
      const active = activeNames();
      return refetch.filter((name) => active.has(name));
    },
  });

  const run = useCallback(
    async (variables: TVariables, alsoEvict: EntityIds = []): Promise<TData> => {
      // Apollo types `mutate`'s argument as a conditional tuple over a
      // generic `TVariables`, which TS cannot resolve here; this is its
      // concrete shape for a mutation that has variables.
      const call = mutate as unknown as (
        options: useMutation.MutationFunctionOptions<TData, TVariables> & { variables: TVariables },
      ) => ReturnType<typeof mutate>;
      const { data } = await call({
        variables,
        update: (cache, result) => {
          if (!result.data) return;
          const written = result.data as TData;
          const active = activeNames();
          evictStale(
            cache,
            [...(evictRef.current?.(written, variables) ?? []), ...alsoEvict],
            refetchRef.current.filter((name) => !active.has(name)),
          );
        },
      });
      if (data === undefined || data === null) throw new Error('The server returned no data');
      return data as TData;
    },
    [mutate, activeNames],
  );

  return [run, { loading, error }] as const;
}
