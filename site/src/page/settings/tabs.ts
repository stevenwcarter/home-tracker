/** The settings screen's tabs, in tab-bar order; each is `/settings/<id>`. */
export const SETTINGS_TABS = [{ id: 'ai', label: 'AI' }] as const;

export type SettingsTabId = (typeof SETTINGS_TABS)[number]['id'];
