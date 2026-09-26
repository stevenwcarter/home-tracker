import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Thumb } from '../Thumb';
import { thumbUrlAt } from 'utils/thumbUrl';

describe('thumbUrlAt', () => {
  it('substitutes the /thumb/<n> size and keeps the query string', () => {
    expect(thumbUrlAt('/attachments/a/thumb/500?v=abc', 300)).toBe(
      '/attachments/a/thumb/300?v=abc',
    );
  });

  it('works without a query string', () => {
    expect(thumbUrlAt('/attachments/a/thumb/300', 1200)).toBe('/attachments/a/thumb/1200');
  });
});

describe('Thumb', () => {
  it('renders a lazy img at the requested size with the title as alt', () => {
    render(
      <Thumb
        attachment={{ thumbnailUrl: '/attachments/a/thumb/500?v=abc', title: 'Drill' }}
        size={300}
      />,
    );
    const img = screen.getByRole('img', { name: 'Drill' });
    expect(img).toHaveAttribute('src', '/attachments/a/thumb/300?v=abc');
    expect(img).toHaveAttribute('loading', 'lazy');
  });

  it('renders a placeholder when there is no thumbnail', () => {
    render(<Thumb attachment={{ thumbnailUrl: null, title: 'Drill' }} size={300} />);
    expect(screen.queryByRole('img', { name: 'Drill' })).not.toBeInTheDocument();
    expect(screen.getByText('No photo')).toHaveClass('text-muted');
  });
});
