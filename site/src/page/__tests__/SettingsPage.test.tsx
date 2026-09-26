import { describe, it, expect } from 'vitest';
import { screen } from '@testing-library/react';
import { aiSettingsMock } from 'test/aiFixtures';
import { renderRoute } from 'test/renderRoute';

describe('SettingsPage', () => {
  it('redirects /settings to the AI tab', async () => {
    renderRoute('/settings', [aiSettingsMock()]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Settings' })).toBeVisible();
    expect(screen.getByTestId('current-path')).toHaveTextContent('/settings/ai');
    expect(await screen.findByLabelText('Base URL')).toBeInTheDocument();
  });

  it('marks the AI tab selected in the tab bar', async () => {
    renderRoute('/settings/ai', [aiSettingsMock()]);
    const tablist = await screen.findByRole('tablist');
    expect(tablist).toBeInTheDocument();
    const tab = screen.getByRole('tab', { name: 'AI' });
    expect(tab).toHaveAttribute('aria-selected', 'true');
    expect(tab).toHaveAttribute('href', '/settings/ai');
    expect(screen.getByRole('tabpanel', { name: 'AI' })).toBeInTheDocument();
  });

  it('shows NotFound for an unknown tab', async () => {
    renderRoute('/settings/nope', []);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeVisible();
    expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
  });
});
