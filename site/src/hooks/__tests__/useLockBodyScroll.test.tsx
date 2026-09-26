import { describe, it, expect, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useLockBodyScroll } from '../useLockBodyScroll';

afterEach(() => {
  document.body.style.overflow = '';
});

describe('useLockBodyScroll', () => {
  it('hides body overflow while active and restores the previous value after', () => {
    document.body.style.overflow = 'scroll';
    const { rerender, unmount } = renderHook(({ active }) => useLockBodyScroll(active), {
      initialProps: { active: true },
    });
    expect(document.body.style.overflow).toBe('hidden');
    rerender({ active: false });
    expect(document.body.style.overflow).toBe('scroll');
    rerender({ active: true });
    unmount();
    expect(document.body.style.overflow).toBe('scroll');
  });

  it('does nothing while inactive', () => {
    renderHook(() => useLockBodyScroll(false));
    expect(document.body.style.overflow).toBe('');
  });

  it('keeps the lock until the last of two overlapping holders lets go', () => {
    const first = renderHook(() => useLockBodyScroll(true));
    const second = renderHook(() => useLockBodyScroll(true));
    first.unmount();
    expect(document.body.style.overflow).toBe('hidden');
    second.unmount();
    expect(document.body.style.overflow).toBe('');
  });
});
