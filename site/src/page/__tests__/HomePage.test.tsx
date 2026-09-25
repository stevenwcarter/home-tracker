import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import HomePage from '../HomePage';
import { GET_SUMMARY } from 'hooks/queries';

const summary = {
  totalValueCents: 1234567,
  currency: 'USD',
  totalItems: 42,
  totalLocations: 7,
  totalTags: 5,
};

describe('HomePage', () => {
  it('renders the four statistics', async () => {
    render(
      <MockedProvider mocks={[{ request: { query: GET_SUMMARY }, result: { data: { summary } } }]}>
        <HomePage />
      </MockedProvider>,
    );
    expect(await screen.findByText('$12,345.67')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
    expect(screen.getByText('7')).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();
    for (const label of ['Total Value', 'Total Items', 'Total Locations', 'Total Tags']) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it('shows an error state instead of endless loading skeletons when the query fails', async () => {
    render(
      <MockedProvider mocks={[{ request: { query: GET_SUMMARY }, error: new Error('boom') }]}>
        <HomePage />
      </MockedProvider>,
    );

    expect(await screen.findByText('Could not load statistics.')).toBeInTheDocument();
    expect(screen.getAllByText('unavailable')).toHaveLength(4);
    expect(screen.queryByLabelText(/ loading$/)).not.toBeInTheDocument();
  });
});
