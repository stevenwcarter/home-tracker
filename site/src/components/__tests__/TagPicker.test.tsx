import { describe, it, expect, vi, beforeEach } from 'vitest';
import { FormEvent, useState } from 'react';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TagPicker } from '../TagPicker';
import { CREATE_TAG } from 'hooks/queries';
import { TAGS, renderWithApollo, tagWire, tagsMock } from 'test/formFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

/** A TagPicker holding its own selection, reporting every change to `onChange`. */
const Harness = ({
  initial,
  onChange,
}: {
  initial: string[];
  onChange: (ids: string[]) => void;
}) => {
  const [value, setValue] = useState(initial);
  return (
    <TagPicker
      value={value}
      onChange={(ids) => {
        setValue(ids);
        onChange(ids);
      }}
    />
  );
};

const kitchen = tagWire('kitchen', 'Kitchen');
const createKitchen = {
  request: {
    query: CREATE_TAG,
    variables: {
      input: { name: 'Kitchen', description: null, color: null, icon: null, parentId: null },
    },
  },
  result: { data: { createTag: kitchen } },
};

describe('TagPicker', () => {
  it('shows a checkbox per tag, checked from the value', async () => {
    renderWithApollo(<Harness initial={['garden']} onChange={vi.fn()} />, [tagsMock()]);
    expect(await screen.findByRole('checkbox', { name: 'Garden' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Power tools' })).not.toBeChecked();
    expect(screen.getByRole('group', { name: 'Tags' })).toBeInTheDocument();
  });

  it('toggles a tag in and out of the value', async () => {
    const onChange = vi.fn();
    renderWithApollo(<Harness initial={['garden']} onChange={onChange} />, [tagsMock()]);
    await userEvent.click(await screen.findByRole('checkbox', { name: 'Power tools' }));
    expect(onChange).toHaveBeenLastCalledWith(['garden', 'power']);
    await userEvent.click(screen.getByRole('checkbox', { name: 'Garden' }));
    expect(onChange).toHaveBeenLastCalledWith(['power']);
  });

  it('creates a new tag inline and selects it', async () => {
    const onChange = vi.fn();
    renderWithApollo(<Harness initial={['garden']} onChange={onChange} />, [
      tagsMock(),
      createKitchen,
      tagsMock([...TAGS, kitchen]),
    ]);
    await screen.findByRole('checkbox', { name: 'Garden' });
    await userEvent.type(screen.getByLabelText('New tag'), ' Kitchen ');
    await userEvent.click(screen.getByRole('button', { name: 'Add tag' }));
    await waitFor(() => expect(onChange).toHaveBeenLastCalledWith(['garden', 'kitchen']));
    expect(await screen.findByRole('checkbox', { name: 'Kitchen' })).toBeChecked();
    expect(screen.getByLabelText('New tag')).toHaveValue('');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('creates on Enter without submitting a surrounding form', async () => {
    const onSubmit = vi.fn((event: FormEvent) => event.preventDefault());
    const onChange = vi.fn();
    renderWithApollo(
      <form onSubmit={onSubmit}>
        <Harness initial={[]} onChange={onChange} />
      </form>,
      [tagsMock(), createKitchen, tagsMock([...TAGS, kitchen])],
    );
    await screen.findByRole('checkbox', { name: 'Garden' });
    await userEvent.type(screen.getByLabelText('New tag'), 'Kitchen{Enter}');
    await waitFor(() => expect(onChange).toHaveBeenLastCalledWith(['kitchen']));
    expect(onSubmit).not.toHaveBeenCalled();
    await screen.findByRole('checkbox', { name: 'Kitchen' });
  });

  it('selects an existing tag of the same name instead of creating a duplicate', async () => {
    const onChange = vi.fn();
    renderWithApollo(<Harness initial={[]} onChange={onChange} />, [tagsMock()]);
    await screen.findByRole('checkbox', { name: 'Garden' });
    await userEvent.type(screen.getByLabelText('New tag'), 'garden');
    await userEvent.click(screen.getByRole('button', { name: 'Add tag' }));
    expect(onChange).toHaveBeenLastCalledWith(['garden']);
    expect(screen.getByLabelText('New tag')).toHaveValue('');
  });

  it('disables Add tag while the new name is blank', async () => {
    renderWithApollo(<Harness initial={[]} onChange={vi.fn()} />, [tagsMock()]);
    await screen.findByRole('checkbox', { name: 'Garden' });
    expect(screen.getByRole('button', { name: 'Add tag' })).toBeDisabled();
    await userEvent.type(screen.getByLabelText('New tag'), '   ');
    expect(screen.getByRole('button', { name: 'Add tag' })).toBeDisabled();
  });
});
