import { ComponentType } from 'react';
import { Link, useParams } from 'react-router-dom';
import { NotFound } from 'page/NotFound';
import { AiSettingsTab } from 'page/settings/AiSettingsTab';
import { SETTINGS_TABS, SettingsTabId } from 'page/settings/tabs';

const PANELS: Record<SettingsTabId, ComponentType> = { ai: AiSettingsTab };

const tabId = (id: string) => `settings-tab-${id}`;

/** `/settings/:tab`: a tab bar over the settings sections; an unknown tab is not found. */
export const SettingsPage = () => {
  const { tab } = useParams();
  const active = SETTINGS_TABS.find(({ id }) => id === tab);
  if (!active) return <NotFound />;
  const Panel = PANELS[active.id];

  return (
    <section>
      <h1>Settings</h1>
      <div
        role="tablist"
        aria-label="Settings sections"
        className="mt-4 flex gap-1 border-b border-border"
      >
        {SETTINGS_TABS.map(({ id, label }) => {
          const selected = id === active.id;
          return (
            <Link
              key={id}
              id={tabId(id)}
              role="tab"
              aria-selected={selected}
              aria-controls={`${tabId(id)}-panel`}
              to={`/settings/${id}`}
              className={`-mb-px border-b-2 px-3 py-2 text-sm ${
                selected
                  ? 'border-accent text-accent'
                  : 'border-transparent text-muted hover:text-text'
              }`}
            >
              {label}
            </Link>
          );
        })}
      </div>
      <div
        role="tabpanel"
        id={`${tabId(active.id)}-panel`}
        aria-labelledby={tabId(active.id)}
        className="mt-6"
      >
        <Panel />
      </div>
    </section>
  );
};

export default SettingsPage;
