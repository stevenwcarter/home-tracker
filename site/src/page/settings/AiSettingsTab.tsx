import { FormEvent, ReactNode, useState } from 'react';
import { PRIMARY_ACTION, SECONDARY_ACTION } from 'components/buttonStyles';
import { FieldControlProps, FormField, INPUT_CLASS } from 'components/FormField';
import { useAiSettings } from 'hooks/useAiSettings';
import { useTestAiConnection, useUpdateAiSettings } from 'hooks/useAiSettingsMutations';
import { AiSettings, AiSettingsInput, AiTestResult } from 'types/ai';

const API_KEY_VAR = 'OPENAI_API_KEY';
const BASE_URL_VAR = 'OPENAI_BASE_URL';

interface Draft {
  baseUrl: string;
  visionModel: string;
  synthesisModel: string;
  extraInstructions: string;
  /** A new key typed in; blank leaves the saved one alone. */
  apiKey: string;
  /** "Clear key" was pressed: the save sends `""`. */
  clearKey: boolean;
}

const draftOf = (settings: AiSettings): Draft => ({
  baseUrl: settings.baseUrl,
  visionModel: settings.visionModel,
  synthesisModel: settings.synthesisModel,
  extraInstructions: settings.extraInstructions ?? '',
  apiKey: '',
  clearKey: false,
});

/**
 * The key goes out only when it changes: omitted to keep the saved one (and
 * always when the environment supplies it), `""` to clear it, else the new key.
 */
const inputOf = (draft: Draft, keyFromEnv: boolean): AiSettingsInput => {
  const input: AiSettingsInput = {
    baseUrl: draft.baseUrl.trim(),
    visionModel: draft.visionModel.trim(),
    synthesisModel: draft.synthesisModel.trim(),
    extraInstructions: draft.extraInstructions.trim() || null,
  };
  if (keyFromEnv) return input;
  if (draft.clearKey) return { ...input, apiKey: '' };
  const typed = draft.apiKey.trim();
  return typed ? { ...input, apiKey: typed } : input;
};

const envNote = (variable: string) => `Set from ${variable} in the environment`;

/** A `FormField` with a helper line under the control, announced as its description. */
const HelpedField = ({
  label,
  help,
  className,
  children,
}: {
  label: string;
  help: ReactNode;
  className?: string;
  children: (control: FieldControlProps) => ReactNode;
}) => (
  <FormField label={label} className={className}>
    {(control) => {
      const helpId = `${control.id}-help`;
      return (
        <>
          {children({ ...control, 'aria-describedby': help ? helpId : undefined })}
          {help && (
            <p id={helpId} className="mt-1 text-sm text-muted">
              {help}
            </p>
          )}
        </>
      );
    }}
  </FormField>
);

const AiSettingsForm = ({ settings }: { settings: AiSettings }) => {
  const { update, loading: saving } = useUpdateAiSettings();
  const { test, loading: testing } = useTestAiConnection();
  const [draft, setDraft] = useState(() => draftOf(settings));
  const [result, setResult] = useState<AiTestResult | null>(null);
  const keyFromEnv = settings.fromEnvironment.includes(API_KEY_VAR);
  const baseUrlFromEnv = settings.fromEnvironment.includes(BASE_URL_VAR);
  const incomplete = [draft.baseUrl, draft.visionModel, draft.synthesisModel].some(
    (value) => value.trim() === '',
  );

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (incomplete || saving) return;
    const saved = await update(inputOf(draft, keyFromEnv));
    // The key never comes back, so the field blanks whether or not the rest changed.
    if (saved) setDraft({ ...draftOf(saved), apiKey: '', clearKey: false });
  };

  const runTest = async () => {
    setResult(null);
    setResult(await test());
  };

  const text = (
    label: string,
    key: 'baseUrl' | 'visionModel' | 'synthesisModel',
    envVar?: string,
  ) => (
    <HelpedField label={label} help={envVar && envNote(envVar)}>
      {(control) => (
        <input
          {...control}
          type="text"
          disabled={Boolean(envVar)}
          value={draft[key]}
          onChange={(event) => setDraft({ ...draft, [key]: event.target.value })}
          className={`${INPUT_CLASS} disabled:opacity-60`}
        />
      )}
    </HelpedField>
  );

  const keyHelp = keyFromEnv
    ? envNote(API_KEY_VAR)
    : draft.clearKey
      ? 'The key will be removed when you save.'
      : settings.hasApiKey
        ? 'Key saved. Enter a new key to replace it.'
        : null;

  return (
    <form aria-label="AI settings" onSubmit={submit} noValidate className="grid max-w-2xl gap-4">
      {text('Base URL', 'baseUrl', baseUrlFromEnv ? BASE_URL_VAR : undefined)}
      {text('Vision model', 'visionModel')}
      {text('Synthesis model', 'synthesisModel')}
      <FormField label="Extra instructions">
        {(control) => (
          <textarea
            {...control}
            rows={4}
            value={draft.extraInstructions}
            onChange={(event) => setDraft({ ...draft, extraInstructions: event.target.value })}
            className={INPUT_CLASS}
          />
        )}
      </FormField>
      <div className="flex flex-wrap items-start gap-2">
        <HelpedField label="API key" help={keyHelp} className="min-w-0 flex-1 basis-60">
          {(control) => (
            <input
              {...control}
              type="password"
              autoComplete="off"
              disabled={keyFromEnv}
              value={draft.apiKey}
              onChange={(event) =>
                setDraft({ ...draft, apiKey: event.target.value, clearKey: false })
              }
              className={`${INPUT_CLASS} disabled:opacity-60`}
            />
          )}
        </HelpedField>
        {settings.hasApiKey && !keyFromEnv && (
          <button
            type="button"
            onClick={() => setDraft({ ...draft, apiKey: '', clearKey: true })}
            disabled={draft.clearKey}
            className={`${SECONDARY_ACTION} mt-6`}
          >
            Clear key
          </button>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <button type="submit" disabled={incomplete || saving} className={PRIMARY_ACTION}>
          Save
        </button>
        <button
          type="button"
          onClick={runTest}
          disabled={!settings.hasApiKey || testing}
          className={SECONDARY_ACTION}
        >
          Test connection
        </button>
      </div>
      <div aria-live="polite">
        {testing && <p className="text-sm text-muted">Testing…</p>}
        {result && (
          <p className={`text-sm break-words ${result.ok ? 'text-success' : 'text-danger'}`}>
            {result.message}
          </p>
        )}
      </div>
    </form>
  );
};

/** The AI tab: the provider endpoint, models, prompt extras and API key, plus a connection test. */
export const AiSettingsTab = () => {
  const { settings, loading, error } = useAiSettings();
  if (settings) return <AiSettingsForm settings={settings} />;
  if (loading) return <p className="text-muted">Loading settings…</p>;
  if (error) return <p className="text-danger">Could not load the AI settings.</p>;
  return null;
};

export default AiSettingsTab;
