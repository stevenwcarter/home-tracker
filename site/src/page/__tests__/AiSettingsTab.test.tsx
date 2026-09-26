import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MockedResponse } from '@apollo/client/testing';
import { TEST_AI_CONNECTION, UPDATE_AI_SETTINGS } from 'hooks/queries';
import { AI_SETTINGS, aiSettingsMock } from 'test/aiFixtures';
import { renderRoute } from 'test/renderRoute';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

beforeEach(() => vi.clearAllMocks());

const HOST_CHANGE_NOTE =
  'Changing the endpoint host cleared the saved key. Enter it again to keep using AI.';

/**
 * An `updateAiSettings` mock that accepts any input and records it, so a test
 * can assert whether `apiKey` was sent at all (equality ignores undefined keys).
 */
const updateMock = (answer: object = AI_SETTINGS) => {
  const result = vi.fn((_variables: { input: Record<string, unknown> }) => ({
    data: { updateAiSettings: answer },
  }));
  const mock: MockedResponse = {
    request: { query: UPDATE_AI_SETTINGS, variables: () => true },
    result,
  };
  return { mock, sent: () => result.mock.calls[0][0].input };
};

const testMock = (ok: boolean, message: string): MockedResponse => ({
  request: { query: TEST_AI_CONNECTION },
  result: {
    data: { testAiConnection: { __typename: 'AiTestResult', ok, message, latencyMs: 420 } },
  },
});

const unchanged = {
  baseUrl: AI_SETTINGS.baseUrl,
  visionModel: AI_SETTINGS.visionModel,
  synthesisModel: AI_SETTINGS.synthesisModel,
  extraInstructions: AI_SETTINGS.extraInstructions,
};

/** Renders the AI tab and waits for the form to fill from the query. */
const renderTab = async (mocks: MockedResponse[], settings: object = AI_SETTINGS) => {
  // A second answer serves the refetch after a save.
  renderRoute('/settings/ai', [aiSettingsMock(settings), ...mocks, aiSettingsMock(settings)]);
  await screen.findByDisplayValue(AI_SETTINGS.baseUrl);
};

describe('AiSettingsTab', () => {
  it('fills the form from the query and leaves the key blank', async () => {
    await renderTab([]);
    expect(screen.getByLabelText('Base URL')).toHaveValue(AI_SETTINGS.baseUrl);
    expect(screen.getByLabelText('Vision model')).toHaveValue('gpt-5-mini');
    expect(screen.getByLabelText('Synthesis model')).toHaveValue('gpt-5-mini');
    const extra = screen.getByLabelText('Extra instructions');
    expect(extra.tagName).toBe('TEXTAREA');
    expect(extra).toHaveValue('Prefer UK spellings.');
    const key = screen.getByLabelText('API key');
    expect(key).toHaveAttribute('type', 'password');
    expect(key).toHaveAttribute('autocomplete', 'off');
    expect(key).toHaveValue('');
    expect(key).toHaveAccessibleDescription('Key saved. Enter a new key to replace it.');
    expect(screen.getByRole('button', { name: 'Clear key' })).toBeInTheDocument();
  });

  it('sends no apiKey when the key was not touched', async () => {
    const update = updateMock();
    await renderTab([update.mock]);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update.sent()).toEqual(unchanged));
    expect(update.sent()).not.toHaveProperty('apiKey');
  });

  it('sends a typed key and blanks the field after saving', async () => {
    const update = updateMock();
    await renderTab([update.mock]);
    const user = userEvent.setup();
    await user.type(screen.getByLabelText('API key'), 'sk-new');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update.sent()).toEqual({ ...unchanged, apiKey: 'sk-new' }));
    await waitFor(() => expect(screen.getByLabelText('API key')).toHaveValue(''));
  });

  it('sends an empty key after "Clear key"', async () => {
    const update = updateMock({ ...AI_SETTINGS, hasApiKey: false });
    await renderTab([update.mock]);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Clear key' }));
    expect(screen.getByText('The key will be removed when you save.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update.sent()).toEqual({ ...unchanged, apiKey: '' }));
  });

  it('sends the edited fields, with blank extra instructions as null', async () => {
    const update = updateMock();
    await renderTab([update.mock]);
    const user = userEvent.setup();
    await user.clear(screen.getByLabelText('Vision model'));
    await user.type(screen.getByLabelText('Vision model'), 'llava');
    await user.clear(screen.getByLabelText('Extra instructions'));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(update.sent()).toEqual({
        ...unchanged,
        visionModel: 'llava',
        extraInstructions: null,
      }),
    );
  });

  it('disables environment-provided fields with a note and still sends the rest', async () => {
    const fromEnv = {
      ...AI_SETTINGS,
      fromEnvironment: ['OPENAI_API_KEY', 'OPENAI_BASE_URL'],
    };
    const update = updateMock(fromEnv);
    await renderTab([update.mock], fromEnv);
    const baseUrl = screen.getByLabelText('Base URL');
    const key = screen.getByLabelText('API key');
    expect(baseUrl).toBeDisabled();
    expect(key).toBeDisabled();
    expect(baseUrl).toHaveAccessibleDescription('Set from OPENAI_BASE_URL in the environment');
    expect(key).toHaveAccessibleDescription('Set from OPENAI_API_KEY in the environment');
    expect(screen.queryByRole('button', { name: 'Clear key' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Vision model')).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update.sent()).toEqual(unchanged));
    expect(update.sent()).not.toHaveProperty('apiKey');
  });

  it('shows a successful connection test in text-success', async () => {
    await renderTab([testMock(true, 'gpt-5-mini answered in 420 ms')]);
    await userEvent.click(screen.getByRole('button', { name: 'Test connection' }));
    const result = await screen.findByText('gpt-5-mini answered in 420 ms');
    expect(result).toHaveClass('text-success');
  });

  it('shows a failed connection test in text-danger', async () => {
    await renderTab([testMock(false, 'The provider refused the API key (401)')]);
    await userEvent.click(screen.getByRole('button', { name: 'Test connection' }));
    const result = await screen.findByText('The provider refused the API key (401)');
    expect(result).toHaveClass('text-danger');
  });

  it('sends an empty key when a typed key is then cleared', async () => {
    const update = updateMock({ ...AI_SETTINGS, hasApiKey: false });
    await renderTab([update.mock]);
    const user = userEvent.setup();
    await user.type(screen.getByLabelText('API key'), 'sk-x');
    await user.click(screen.getByRole('button', { name: 'Clear key' }));
    expect(screen.getByLabelText('API key')).toHaveValue('');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update.sent()).toEqual({ ...unchanged, apiKey: '' }));
    // A clear the user asked for is not the endpoint change clearing the key.
    expect(screen.queryByText(HOST_CHANGE_NOTE)).not.toBeInTheDocument();
  });

  it('says so when a save to another endpoint host cleared the key', async () => {
    const elsewhere = 'https://elsewhere.example/v1';
    const update = updateMock({ ...AI_SETTINGS, baseUrl: elsewhere, hasApiKey: false });
    await renderTab([update.mock]);
    const user = userEvent.setup();
    await user.clear(screen.getByLabelText('Base URL'));
    await user.type(screen.getByLabelText('Base URL'), elsewhere);
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update.sent()).toEqual({ ...unchanged, baseUrl: elsewhere }));
    expect(await screen.findByText(HOST_CHANGE_NOTE)).toBeInTheDocument();
    expect(screen.getByLabelText('API key')).toHaveAccessibleDescription(HOST_CHANGE_NOTE);
  });

  it('clears the test result on an edit and on a save', async () => {
    const update = updateMock();
    await renderTab([testMock(true, 'first answer'), testMock(true, 'second answer'), update.mock]);
    const user = userEvent.setup();
    const test = screen.getByRole('button', { name: 'Test connection' });
    await user.click(test);
    expect(await screen.findByText('first answer')).toBeInTheDocument();
    await user.type(screen.getByLabelText('Vision model'), 'x');
    expect(screen.queryByText('first answer')).not.toBeInTheDocument();

    await user.type(screen.getByLabelText('Vision model'), '{Backspace}');
    await user.click(test);
    expect(await screen.findByText('second answer')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update.sent()).toEqual(unchanged));
    expect(screen.queryByText('second answer')).not.toBeInTheDocument();
  });

  it('disables Test connection while there are unsaved changes', async () => {
    await renderTab([]);
    const user = userEvent.setup();
    const test = screen.getByRole('button', { name: 'Test connection' });
    expect(test).toBeEnabled();
    expect(test).not.toHaveAccessibleDescription();

    await user.type(screen.getByLabelText('API key'), 'sk-new');
    expect(test).toBeDisabled();
    expect(test).toHaveAccessibleDescription('Save your changes to test them');

    await user.clear(screen.getByLabelText('API key'));
    expect(test).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Clear key' }));
    expect(test).toBeDisabled();
  });

  it('disables Test connection until a key is saved', async () => {
    await renderTab([], { ...AI_SETTINGS, hasApiKey: false });
    expect(screen.getByRole('button', { name: 'Test connection' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Clear key' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('API key')).not.toHaveAccessibleDescription();
  });
});
