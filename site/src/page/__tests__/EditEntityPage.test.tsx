import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { UPDATE_ENTITY } from 'hooks/queries';
import { entityDetail } from 'test/entityFixtures';
import { locationsMock, lookupMocks, tagsMock, typesMock } from 'test/formFixtures';
import { entityMock, spiedMock, summaryMock } from 'test/pageMocks';
import { renderRoute } from 'test/renderRoute';
import { toEntityInput } from 'utils/entityInput';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const drill = entityDetail({ id: 'drill', name: 'Drill', parentId: 'garage', quantity: 1.5 });
const renamed = { ...drill, name: 'Hammer drill' };

describe('EditEntityPage', () => {
  it('saves the full input and returns to the item page', async () => {
    const user = userEvent.setup();
    const input = { ...toEntityInput(drill), name: 'Hammer drill' };
    const updated = spiedMock(UPDATE_ENTITY, { id: 'drill', input }, { updateEntity: renamed });
    renderRoute('/items/drill/edit', [
      entityMock('drill', drill),
      ...lookupMocks(),
      updated.mock,
      entityMock('drill', renamed),
      typesMock(),
      tagsMock(),
      locationsMock(),
      summaryMock(),
    ]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Edit Drill' })).toBeVisible();
    await screen.findByRole('option', { name: 'Tool' });
    await screen.findByRole('checkbox', { name: 'Garden' });
    await screen.findByRole('option', { name: /Office/ });
    const name = screen.getByLabelText('Name');
    expect(name).toHaveValue('Drill');
    await user.clear(name);
    await user.type(name, 'Hammer drill');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/items\/drill$/),
    );
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Hammer drill' }),
    ).toBeInTheDocument();
    expect(updated.result).toHaveBeenCalledTimes(1);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('Cancel leads back to a location page for a location', async () => {
    const garage = entityDetail({ id: 'garage', name: 'Garage' }, true);
    renderRoute('/locations/garage/edit', [entityMock('garage', garage), ...lookupMocks()]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Edit Garage' })).toBeVisible();
    expect(screen.getByRole('link', { name: 'Cancel' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
  });

  it('shows Not found for an unknown id', async () => {
    renderRoute('/items/missing/edit', [entityMock('missing', null), ...lookupMocks()]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeVisible();
  });
});
