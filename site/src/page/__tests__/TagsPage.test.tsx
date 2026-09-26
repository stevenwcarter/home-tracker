import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CREATE_TAG, DELETE_TAG, UPDATE_TAG } from 'hooks/queries';
import { tagsMock, tagWire } from 'test/formFixtures';
import { spiedMock } from 'test/pageMocks';
import { renderRoute } from 'test/renderRoute';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const tools = { ...tagWire('tools', 'Tools'), color: '#ff8800', entityCount: 2 };
const power = {
  ...tagWire('power', 'Power tools'),
  parent: { __typename: 'Tag', id: 'tools' },
};
const garden = tagWire('garden', 'Garden');
const TAGS = [tools, power, garden];

const row = (name: string) => screen.getByRole('row', { name: new RegExp(`^${name}\\b`) });

describe('TagsPage', () => {
  it('lists tags with a colour swatch, parent and entity count', async () => {
    renderRoute('/tags', [tagsMock(TAGS)]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Tags' })).toBeVisible();
    await screen.findByRole('cell', { name: /Power tools/ });
    expect(within(row('Tools')).getByTestId('swatch-tools')).toHaveStyle({
      background: '#ff8800',
    });
    expect(within(row('Tools')).getByText('2')).toBeInTheDocument();
    expect(within(row('Power tools')).getByText('Tools')).toBeInTheDocument();
  });

  it('creates a tag with a colour and a parent', async () => {
    const user = userEvent.setup();
    const input = {
      name: 'Hand tools',
      description: null,
      color: '#123456',
      icon: null,
      parentId: 'tools',
    };
    const hand = {
      ...tagWire('hand', 'Hand tools'),
      color: '#123456',
      parent: { __typename: 'Tag', id: 'tools' },
    };
    const created = spiedMock(CREATE_TAG, { input }, { createTag: hand });
    renderRoute('/tags', [tagsMock(TAGS), created.mock, tagsMock([...TAGS, hand])]);
    const form = await screen.findByRole('form', { name: 'New tag' });
    await screen.findByRole('cell', { name: /Garden/ });
    await user.type(within(form).getByLabelText('Name'), 'Hand tools');
    await user.type(within(form).getByLabelText('Colour'), '#123456');
    await user.selectOptions(within(form).getByLabelText('Parent tag'), 'tools');
    await user.click(within(form).getByRole('button', { name: 'Add tag' }));
    expect(await screen.findByRole('cell', { name: /Hand tools/ })).toBeInTheDocument();
    expect(created.result).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(within(form).getByLabelText('Name')).toHaveValue(''));
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('never offers a tag, or its descendants, as its own parent', async () => {
    const user = userEvent.setup();
    renderRoute('/tags', [tagsMock(TAGS)]);
    await screen.findByRole('cell', { name: /Garden/ });
    await user.click(screen.getByRole('button', { name: 'Edit Tools' }));
    const form = screen.getByRole('form', { name: 'Edit Tools' });
    const parent = within(form).getByLabelText('Parent tag');
    const options = within(parent)
      .getAllByRole('option')
      .map((option) => option.textContent);
    expect(options).toEqual(['None', 'Garden']);
  });

  it('edits a tag in place', async () => {
    const user = userEvent.setup();
    const input = { name: 'Yard', description: null, color: null, icon: null, parentId: null };
    const yard = { ...garden, name: 'Yard' };
    const updated = spiedMock(UPDATE_TAG, { id: 'garden', input }, { updateTag: yard });
    renderRoute('/tags', [tagsMock(TAGS), updated.mock, tagsMock([tools, power, yard])]);
    await screen.findByRole('cell', { name: /Garden/ });
    await user.click(screen.getByRole('button', { name: 'Edit Garden' }));
    const form = screen.getByRole('form', { name: 'Edit Garden' });
    const name = within(form).getByLabelText('Name');
    await user.clear(name);
    await user.type(name, 'Yard');
    await user.click(within(form).getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('cell', { name: /^Yard/ })).toBeInTheDocument();
    expect(updated.result).toHaveBeenCalledTimes(1);
  });

  it('deletes a tag after confirming, naming how many entities lose it', async () => {
    const user = userEvent.setup();
    const deleted = spiedMock(DELETE_TAG, { id: 'tools' }, { deleteTag: true });
    renderRoute('/tags', [tagsMock(TAGS), deleted.mock, tagsMock([garden])]);
    await screen.findByRole('cell', { name: /Garden/ });
    await user.click(screen.getByRole('button', { name: 'Delete Tools' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete Tools?' });
    expect(dialog).toHaveTextContent('It will be removed from 2 entities.');
    await user.click(within(dialog).getByRole('button', { name: 'Delete tag' }));
    await waitFor(() =>
      expect(screen.queryByRole('cell', { name: /Tools/ })).not.toBeInTheDocument(),
    );
    expect(deleted.result).toHaveBeenCalledTimes(1);
  });
});
