import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TagChips } from '../TagChips';

describe('TagChips', () => {
  it('renders one chip per tag in a labelled list', () => {
    render(
      <TagChips
        tags={[
          { id: 't1', name: 'Power tools', color: '#ff0000' },
          { id: 't2', name: 'Garage', color: null },
        ]}
      />,
    );
    const list = screen.getByRole('list', { name: 'Tags' });
    expect(list).toHaveTextContent('Power tools');
    expect(list).toHaveTextContent('Garage');
  });

  it('draws a colour dot only for a valid hex colour', () => {
    render(
      <TagChips
        tags={[
          { id: 't1', name: 'Hex', color: '#ff0000' },
          { id: 't2', name: 'Named', color: 'red' },
          { id: 't3', name: 'Junk', color: 'url(https://example.com/x.png)' },
        ]}
      />,
    );
    const chip = (name: string) => screen.getByText(name).closest('li') as HTMLElement;
    expect(chip('Hex').querySelector('[aria-hidden="true"]')).toHaveStyle({
      backgroundColor: '#ff0000',
    });
    expect(chip('Named').querySelector('[aria-hidden="true"]')).toBeNull();
    expect(chip('Junk').querySelector('[aria-hidden="true"]')).toBeNull();
  });

  it('renders nothing without tags', () => {
    const { container } = render(<TagChips tags={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});
