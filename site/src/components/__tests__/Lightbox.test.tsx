import { describe, it, expect, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { Lightbox } from '../Lightbox';
import { photoRef } from 'test/photoFixtures';

const PHOTOS = [
  photoRef({ id: 'p1', title: 'Front' }),
  photoRef({ id: 'p2', title: 'Side' }),
  photoRef({ id: 'p3', title: 'Back' }),
];

const renderLightbox = (startIndex = 0, photos = PHOTOS) => {
  const onClose = vi.fn();
  render(<Lightbox photos={photos} startIndex={startIndex} onClose={onClose} />);
  return { onClose };
};

const shownImage = () => screen.getByRole('img');

describe('Lightbox', () => {
  it('is a labelled modal dialog showing the photo at the 1200 size', () => {
    renderLightbox(1);
    const dialog = screen.getByRole('dialog', { name: 'Side' });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(shownImage()).toHaveAttribute('src', '/attachments/p2/thumb/1200?v=abc');
    expect(dialog).toHaveTextContent('2 of 3');
    expect(screen.getByRole('link', { name: 'Open original' })).toHaveAttribute(
      'href',
      '/attachments/p2?v=abc',
    );
  });

  it('moves focus into the dialog when it opens', () => {
    renderLightbox();
    expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus();
  });

  it('Next and Previous step through the photos and wrap at the ends', async () => {
    const user = userEvent.setup();
    renderLightbox(2);
    await user.click(screen.getByRole('button', { name: 'Next photo' }));
    expect(shownImage()).toHaveAttribute('src', '/attachments/p1/thumb/1200?v=abc');
    expect(screen.getByRole('dialog', { name: 'Front' })).toHaveTextContent('1 of 3');
    await user.click(screen.getByRole('button', { name: 'Previous photo' }));
    expect(shownImage()).toHaveAttribute('src', '/attachments/p3/thumb/1200?v=abc');
    await user.click(screen.getByRole('button', { name: 'Previous photo' }));
    expect(shownImage()).toHaveAttribute('src', '/attachments/p2/thumb/1200?v=abc');
  });

  it('steps with the arrow keys', async () => {
    const user = userEvent.setup();
    renderLightbox(0);
    await user.keyboard('{ArrowRight}');
    expect(shownImage()).toHaveAttribute('src', '/attachments/p2/thumb/1200?v=abc');
    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(shownImage()).toHaveAttribute('src', '/attachments/p3/thumb/1200?v=abc');
  });

  it('has no Previous or Next for a single photo', () => {
    renderLightbox(0, [PHOTOS[0]]);
    expect(screen.queryByRole('button', { name: 'Next photo' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Previous photo' })).not.toBeInTheDocument();
    expect(screen.queryByText('1 of 1')).not.toBeInTheDocument();
  });

  it('closes on Escape, the Close button and a backdrop click', async () => {
    const user = userEvent.setup();
    const { onClose } = renderLightbox();
    await user.keyboard('{Escape}');
    await user.click(screen.getByRole('button', { name: 'Close' }));
    await user.click(screen.getByTestId('lightbox-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(3);
  });

  it('marks Escape handled, so a bubble-phase listener (the drawer) skips it', async () => {
    const user = userEvent.setup();
    const drawer = vi.fn();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !event.defaultPrevented) drawer();
    };
    document.addEventListener('keydown', onKeyDown);
    try {
      const { onClose } = renderLightbox();
      await user.keyboard('{Escape}');
      expect(onClose).toHaveBeenCalledTimes(1);
      expect(drawer).not.toHaveBeenCalled();
    } finally {
      document.removeEventListener('keydown', onKeyDown);
    }
  });

  it('ignores an Escape something earlier already handled', () => {
    const { onClose } = renderLightbox();
    const handledFirst = (event: KeyboardEvent) => event.preventDefault();
    window.addEventListener('keydown', handledFirst, { capture: true });
    try {
      fireEvent.keyDown(document.body, { key: 'Escape' });
      expect(onClose).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener('keydown', handledFirst, { capture: true });
    }
  });

  it('keeps Tab focus inside the dialog', async () => {
    const user = userEvent.setup();
    renderLightbox();
    const inside = screen.getByRole('dialog');
    for (let step = 0; step < 6; step += 1) {
      await user.tab();
      expect(inside).toContainElement(document.activeElement as HTMLElement);
    }
    await user.tab({ shift: true });
    expect(inside).toContainElement(document.activeElement as HTMLElement);
  });

  it('returns focus to the opener when it closes', async () => {
    const user = userEvent.setup();
    const Harness = () => {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            View Front
          </button>
          {open && <Lightbox photos={PHOTOS} startIndex={0} onClose={() => setOpen(false)} />}
        </>
      );
    };
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'View Front' }));
    expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'View Front' })).toHaveFocus();
  });

  // Safari does not focus a button on mouse click, so the opener is passed in
  // rather than read from `document.activeElement`.
  it('returns focus to the given opener even when it never had focus', async () => {
    const user = userEvent.setup();
    const thumb = document.createElement('button');
    thumb.textContent = 'Thumb';
    document.body.appendChild(thumb);
    try {
      const Harness = () => {
        const [open, setOpen] = useState(true);
        return open ? (
          <Lightbox photos={PHOTOS} startIndex={0} opener={thumb} onClose={() => setOpen(false)} />
        ) : (
          <p>closed</p>
        );
      };
      render(<Harness />);
      // Focus was never on the thumb (document.body had it at open).
      expect(thumb).not.toHaveFocus();
      await user.keyboard('{Escape}');
      expect(screen.getByText('closed')).toBeInTheDocument();
      expect(thumb).toHaveFocus();
    } finally {
      thumb.remove();
    }
  });

  it('locks page scrolling while open', () => {
    const { unmount } = render(<Lightbox photos={PHOTOS} startIndex={0} onClose={vi.fn()} />);
    expect(document.body.style.overflow).toBe('hidden');
    unmount();
    expect(document.body.style.overflow).toBe('');
  });
});
