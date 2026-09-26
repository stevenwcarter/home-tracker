import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { IngestProgress } from '../IngestProgress';
import { ingestBatch, ingestItem } from 'test/ingestFixtures';

describe('IngestProgress', () => {
  it('shows one chip per item with its status, politely announced', () => {
    const statuses = ['QUEUED', 'ANALYSING', 'READY', 'FAILED', 'ACCEPTED', 'SKIPPED'] as const;
    render(
      <IngestProgress
        batch={ingestBatch({
          id: 'b1',
          status: 'REVIEWING',
          items: statuses.map((status, index) =>
            ingestItem({ id: `i${index}`, position: index * 2, status }),
          ),
        })}
        currentId="i2"
      />,
    );
    const strip = screen.getByRole('list', { name: 'Progress' });
    expect(strip).toHaveAttribute('aria-live', 'polite');
    expect(strip).toHaveClass('flex-wrap');
    const chips = within(strip).getAllByRole('listitem');
    expect(chips.map((chip) => chip.textContent)).toEqual([
      'Item 1: Queued',
      'Item 2: Analysing',
      'Item 3: Ready',
      'Item 4: Failed',
      'Item 5: Saved',
      'Item 6: Skipped',
    ]);
    expect(chips[2]).toHaveAttribute('aria-current', 'step');
    expect(chips[0]).not.toHaveAttribute('aria-current');
  });
});
