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

  it('renders nothing without tags', () => {
    const { container } = render(<TagChips tags={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});
