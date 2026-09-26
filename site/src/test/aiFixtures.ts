import type { MockedResponse } from '@apollo/client/testing';
import { GET_AI_SETTINGS } from 'hooks/queries';

/** `aiSettings` as the server answers it: a saved key, nothing from the environment. */
export const AI_SETTINGS = {
  __typename: 'AiSettings',
  baseUrl: 'https://api.openai.com/v1',
  visionModel: 'gpt-5-mini',
  synthesisModel: 'gpt-5-mini',
  extraInstructions: 'Prefer UK spellings.',
  hasApiKey: true,
  fromEnvironment: [] as string[],
};

/** One `GetAiSettings` answer. */
export const aiSettingsMock = (settings: object = AI_SETTINGS): MockedResponse => ({
  request: { query: GET_AI_SETTINGS },
  result: { data: { aiSettings: settings } },
});
