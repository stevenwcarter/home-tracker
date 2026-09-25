import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatCard } from '../StatCard';

describe('StatCard', () => {
  it('renders the label and value', () => {
    render(<StatCard label="Total Items" value="42" />);
    expect(screen.getByText('Total Items')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
  });

  it('shows a placeholder while loading', () => {
    render(<StatCard label="Total Items" value={null} />);
    expect(screen.getByLabelText('Total Items loading')).toBeInTheDocument();
  });
});
