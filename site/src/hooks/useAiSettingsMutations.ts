import { useCallback } from 'react';
import { useMutation } from '@apollo/client/react';
import { AiSettings, AiSettingsInput, AiTestResult } from 'types/ai';
import { TEST_AI_CONNECTION, UPDATE_AI_SETTINGS } from './queries';
import { toastOnFailure } from './mutationToast';
import { useRefetchingMutation } from './useRefetchingMutation';

// `AiSettings` has no id, so the saved answer only reaches the screen through a refetch.
const REFETCH = ['GetAiSettings'];

/** `update(input)` resolves to the saved settings, or null after toasting a failure. */
export const useUpdateAiSettings = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { updateAiSettings: AiSettings },
    { input: AiSettingsInput }
  >(UPDATE_AI_SETTINGS, { refetch: REFETCH });
  const update = useCallback(
    (input: AiSettingsInput): Promise<AiSettings | null> =>
      toastOnFailure(async () => (await mutate({ input })).updateAiSettings, 'save', null),
    [mutate],
  );
  return { update, loading };
};

/**
 * `test()` resolves to the server's verdict (a refused key is an `ok: false`
 * result, not an error), or null after toasting a request that failed outright.
 */
export const useTestAiConnection = () => {
  const [mutate, { loading }] = useMutation<{ testAiConnection: AiTestResult }>(TEST_AI_CONNECTION);
  const test = useCallback(
    (): Promise<AiTestResult | null> =>
      toastOnFailure(
        async () => {
          const { data } = await mutate();
          if (!data) throw new Error('The server returned no data');
          return data.testAiConnection;
        },
        'test the connection',
        null,
      ),
    [mutate],
  );
  return { test, loading };
};
