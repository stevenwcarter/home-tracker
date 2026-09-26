import type { ApolloCache, DocumentNode, OperationVariables } from '@apollo/client';
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

type EntityIds = ReadonlyArray<string | null | undefined>;

export interface RefetchingMutationOptions<TData, TVariables> {
  /** Operation names to refetch once the mutation succeeds; only active ones are refetched. */
  refetch: readonly string[];
  /** Ids of `Entity` cache entries the result makes stale (a parent whose children changed, a deleted entity). */
  evict?: (data: TData, variables: TVariables) => EntityIds;
}

/** Drops each `Entity:<id>` entry, then garbage-collects what only they referenced. */
function evictEntities(cache: ApolloCache, ids: EntityIds): void {
  let evicted = false;
  for (const id of ids) {
    if (!id) continue;
    cache.evict({ id: cache.identify({ __typename: 'Entity', id }) });
    evicted = true;
  }
  if (evicted) cache.gc();
}

/**
 * `useMutation` plus this app's refetch policy. On success it evicts the
 * `Entity` entries named by `evict` (and by the call's `alsoEvict`, for ids
 * only the caller knows, such as a moved entity's old parent), then refetches
 * the named queries that are currently active, and resolves only once those
 * refetches land (`awaitRefetchQueries`).
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
  useEffect(() => {
    evictRef.current = evict;
  }, [evict]);
  const [mutate, { loading, error }] = useMutation<TData, TVariables>(mutation, {
    awaitRefetchQueries: true,
    // Resolved at completion, not render: naming an inactive query makes Apollo warn.
    refetchQueries: () => {
      const active = new Set(
        [...client.getObservableQueries('active')].map((query) => query.queryName),
      );
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
          evictEntities(cache, [...(evictRef.current?.(written, variables) ?? []), ...alsoEvict]);
        },
      });
      if (data === undefined || data === null) throw new Error('The server returned no data');
      return data as TData;
    },
    [mutate],
  );

  return [run, { loading, error }] as const;
}
