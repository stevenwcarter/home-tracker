import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useAiSettings } from '../useAiSettings';
import { useTestAiConnection, useUpdateAiSettings } from '../useAiSettingsMutations';
import { GET_AI_SETTINGS, TEST_AI_CONNECTION, UPDATE_AI_SETTINGS } from '../queries';
import { AI_SETTINGS, aiSettingsMock } from 'test/aiFixtures';
import { spiedMock } from 'test/pageMocks';
import { AiSettingsInput } from 'types/ai';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const input: AiSettingsInput = {
  baseUrl: 'http://llm.lan:8080/v1',
  visionModel: 'llava',
  synthesisModel: 'llama3',
  extraInstructions: null,
};
const SAVED = { ...AI_SETTINGS, ...input };

describe('useUpdateAiSettings', () => {
  it('resolves the saved settings and refetches GetAiSettings', async () => {
    const refetch = spiedMock(GET_AI_SETTINGS, undefined, { aiSettings: SAVED });
    const mocks = [
      aiSettingsMock(),
      {
        request: { query: UPDATE_AI_SETTINGS, variables: { input } },
        result: { data: { updateAiSettings: SAVED } },
      },
      refetch.mock,
    ];
    const { result } = renderHook(
      () => ({ hook: useUpdateAiSettings(), settings: useAiSettings().settings }),
      { wrapper: wrapper(mocks) },
    );
    await waitFor(() => expect(result.current.settings?.baseUrl).toBe(AI_SETTINGS.baseUrl));
    let saved: unknown;
    await act(async () => {
      saved = await result.current.hook.update(input);
    });
    expect(saved).toEqual(SAVED);
    expect(refetch.result).toHaveBeenCalledTimes(1);
    expect(result.current.settings?.baseUrl).toBe(input.baseUrl);
  });

  it('resolves null and toasts "Could not save" on failure', async () => {
    const mocks = [
      {
        request: { query: UPDATE_AI_SETTINGS, variables: { input } },
        result: { errors: [{ message: 'vision model must not be blank' }] },
      },
    ];
    const { result } = renderHook(() => useUpdateAiSettings(), { wrapper: wrapper(mocks) });
    let saved: unknown;
    await act(async () => {
      saved = await result.current.update(input);
    });
    expect(saved).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not save: vision model must not be blank');
  });
});

describe('useTestAiConnection', () => {
  const answer = {
    __typename: 'AiTestResult',
    ok: true,
    message: 'gpt-5-mini answered in 420 ms',
    latencyMs: 420,
  };

  it('resolves the test result', async () => {
    const mocks = [
      { request: { query: TEST_AI_CONNECTION }, result: { data: { testAiConnection: answer } } },
    ];
    const { result } = renderHook(() => useTestAiConnection(), { wrapper: wrapper(mocks) });
    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.test();
    });
    expect(outcome).toEqual(answer);
  });

  it('resolves null and toasts when the request itself fails', async () => {
    const mocks = [
      { request: { query: TEST_AI_CONNECTION }, result: { errors: [{ message: 'forbidden' }] } },
    ];
    const { result } = renderHook(() => useTestAiConnection(), { wrapper: wrapper(mocks) });
    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.test();
    });
    expect(outcome).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not test the connection: forbidden');
  });
});
