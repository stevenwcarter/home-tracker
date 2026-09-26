/** The AI settings as the server reports them; the key itself never leaves the server. */
export interface AiSettings {
  baseUrl: string;
  visionModel: string;
  synthesisModel: string;
  extraInstructions: string | null;
  hasApiKey: boolean;
  /** The environment variables overriding a field, e.g. `OPENAI_API_KEY`. */
  fromEnvironment: string[];
}

/** An `updateAiSettings` input. `apiKey` omitted keeps the saved key, `""` clears it, otherwise replaces it. */
export interface AiSettingsInput {
  baseUrl: string;
  visionModel: string;
  synthesisModel: string;
  extraInstructions: string | null;
  apiKey?: string | null;
}

/** The outcome of `testAiConnection`. */
export interface AiTestResult {
  ok: boolean;
  /** Which model answered and how fast, or why the call failed. */
  message: string;
  latencyMs: number;
}
