import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ConfirmDialog } from '../ConfirmDialog';

const renderDialog = (open = true) => {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  const utils = render(
    <>
      <button type="button">Delete</button>
      <ConfirmDialog
        open={open}
        title="Delete Drill?"
        body="This cannot be undone."
        confirmLabel="Delete item"
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    </>,
  );
  return { ...utils, onConfirm, onCancel };
};

describe('ConfirmDialog', () => {
  it('renders nothing while closed', () => {
    renderDialog(false);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('is a modal dialog named by its title, showing the body', () => {
    renderDialog();
    const dialog = screen.getByRole('dialog', { name: 'Delete Drill?' });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(dialog).toHaveTextContent('This cannot be undone.');
  });

  it('moves focus to the Cancel button when it opens', () => {
    renderDialog();
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
  });

  it('cancels on Escape', async () => {
    const { onCancel, onConfirm } = renderDialog();
    await userEvent.keyboard('{Escape}');
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it('cancels on a backdrop click', async () => {
    const { onCancel } = renderDialog();
    await userEvent.click(screen.getByTestId('confirm-dialog-backdrop'));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('cancels from the Cancel button and confirms from the confirm button', async () => {
    const { onCancel, onConfirm } = renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole('button', { name: 'Delete item' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('keeps Tab focus inside the dialog', async () => {
    renderDialog();
    await userEvent.tab();
    expect(screen.getByRole('button', { name: 'Delete item' })).toHaveFocus();
    await userEvent.tab();
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(screen.getByRole('button', { name: 'Delete item' })).toHaveFocus();
  });

  it('returns focus to the opener when it closes', () => {
    const dialog = (open: boolean) => (
      <>
        <button type="button">Delete</button>
        <ConfirmDialog
          open={open}
          title="Delete Drill?"
          body="Gone."
          confirmLabel="Delete item"
          onConfirm={() => {}}
          onCancel={() => {}}
        />
      </>
    );
    const { rerender } = render(dialog(false));
    screen.getByRole('button', { name: 'Delete' }).focus();
    rerender(dialog(true));
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    rerender(dialog(false));
    expect(screen.getByRole('button', { name: 'Delete' })).toHaveFocus();
  });

  it('locks page scrolling while open', () => {
    const { rerender, onCancel, onConfirm } = renderDialog();
    expect(document.body.style.overflow).toBe('hidden');
    rerender(
      <ConfirmDialog
        open={false}
        title="Delete Drill?"
        body="This cannot be undone."
        confirmLabel="Delete item"
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );
    expect(document.body.style.overflow).toBe('');
  });
});
