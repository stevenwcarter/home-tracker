import type { DocumentNode } from 'graphql';
import type { MockedResponse } from '@apollo/client/testing';
import { vi } from 'vitest';
import { GET_ENTITY, GET_SUMMARY } from 'hooks/queries';
import { SUMMARY } from './entityFixtures';

/** One `GetEntity` answer for `id` (null for "no such entity"). */
export const entityMock = (id: string, entity: unknown): MockedResponse => ({
  request: { query: GET_ENTITY, variables: { id } },
  result: { data: { entity } },
});

export const summaryMock = (): MockedResponse => ({
  request: { query: GET_SUMMARY },
  result: { data: { summary: SUMMARY } },
});

/**
 * A mock whose `result` is a spy, so a test can assert the operation actually
 * ran (a mutation was sent, a query was refetched).
 */
export const spiedMock = (query: DocumentNode, variables: object | undefined, data: object) => {
  const result = vi.fn(() => ({ data }));
  const mock: MockedResponse = { request: variables ? { query, variables } : { query }, result };
  return { mock, result };
};
