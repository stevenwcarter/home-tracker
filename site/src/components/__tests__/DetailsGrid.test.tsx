import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DetailsGrid } from '../DetailsGrid';

describe('DetailsGrid', () => {
  it('renders each label with its value', () => {
    render(
      <DetailsGrid
        rows={[
          { label: 'Asset ID', value: '000-001' },
          { label: 'Quantity', value: 3 },
        ]}
      />,
    );
    expect(screen.getByText('Asset ID')).toBeInTheDocument();
    expect(screen.getByText('000-001')).toBeInTheDocument();
    expect(screen.getByText('Quantity')).toBeInTheDocument();
    expect(screen.getByText('3')).toBeInTheDocument();
  });

  it('omits rows whose value is null, undefined or blank', () => {
    render(
      <DetailsGrid
        rows={[
          { label: 'Manufacturer', value: 'Makita' },
          { label: 'Model', value: null },
          { label: 'Serial number', value: undefined },
          { label: 'Notes', value: '   ' },
        ]}
      />,
    );
    expect(screen.getByText('Manufacturer')).toBeInTheDocument();
    for (const label of ['Model', 'Serial number', 'Notes']) {
      expect(screen.queryByText(label)).not.toBeInTheDocument();
    }
  });

  it('renders nothing when every value is empty', () => {
    const { container } = render(<DetailsGrid rows={[{ label: 'Model', value: null }]} />);
    expect(container).toBeEmptyDOMElement();
  });
});
