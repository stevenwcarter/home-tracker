-- The AI settings with a default. The API key and the extra instructions have
-- none: a missing row means "not set".
INSERT OR IGNORE INTO settings (key, value) VALUES
  ('ai.base_url', 'https://api.openai.com/v1'),
  ('ai.vision_model', 'gpt-5-mini'),
  ('ai.synthesis_model', 'gpt-5-mini');
