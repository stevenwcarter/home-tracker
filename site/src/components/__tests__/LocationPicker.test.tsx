import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { LocationPicker } from '../LocationPicker';
import { locationsMock, renderWithApollo } from 'test/formFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

beforeEach(() => vi.clearAllMocks());

const renderPicker = (props: Partial<Parameters<typeof LocationPicker>[0]> = {}) => {
  const onChange = vi.fn();
  renderWithApollo(<LocationPicker value={null} onChange={onChange} allowNone {...props} />, [
    locationsMock(),
  ]);
  return { onChange };
};

/** Option labels once the locations have loaded, with the indent left in. */
const optionTexts = async () => {
  await screen.findByRole('option', { name: /Office/ });
  return within(screen.getByLabelText('Parent location'))
    .getAllByRole('option')
    .map((option) => option.textContent);
};

const INDENT = '   ';

describe('LocationPicker', () => {
  it('lists None and every location in tree order, indented by depth', async () => {
    renderPicker();
    expect(await optionTexts()).toEqual([
      'None (top level)',
      'House',
      `${INDENT}Garage`,
      `${INDENT}${INDENT}Shelf`,
      'Office',
    ]);
  });

  it('leaves out None unless allowNone is set', async () => {
    renderPicker({ allowNone: false, value: 'office' });
    expect(await optionTexts()).not.toContain('None (top level)');
  });

  it('never offers the excluded location or any of its descendants', async () => {
    renderPicker({ excludeSubtreeOf: 'garage' });
    expect(await optionTexts()).toEqual(['None (top level)', 'House', 'Office']);
  });

  it('excluding a root drops its whole subtree', async () => {
    renderPicker({ excludeSubtreeOf: 'house' });
    expect(await optionTexts()).toEqual(['None (top level)', 'Office']);
  });

  it('reports the chosen id, and null for None', async () => {
    const { onChange } = renderPicker({ value: 'office' });
    await optionTexts();
    const select = screen.getByLabelText('Parent location');
    expect(select).toHaveValue('office');
    await userEvent.selectOptions(select, 'shelf');
    expect(onChange).toHaveBeenLastCalledWith('shelf');
    await userEvent.selectOptions(select, '');
    expect(onChange).toHaveBeenLastCalledWith(null);
  });

  it('keeps a current parent that is not a listed location selectable', async () => {
    renderPicker({ value: 'toolbox' });
    const texts = await optionTexts();
    expect(texts).toContain('Current parent (not a location)');
    expect(screen.getByLabelText('Parent location')).toHaveValue('toolbox');
  });
});
