import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import React from 'react';
import { ThemeProvider } from '../ThemeProvider';
import { THEME_STORAGE_KEY, useTheme } from '../useTheme';

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <ThemeProvider>{children}</ThemeProvider>
);

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
});

describe('useTheme', () => {
  it('defaults to dark and stamps the html element', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
  });

  it('restores a stored theme', () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'light');
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');
  });

  it('ignores garbage in storage and falls back to dark', () => {
    // Review focus 5.
    localStorage.setItem(THEME_STORAGE_KEY, 'neon');
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('dark');
  });

  it('survives storage that throws', () => {
    // Seed a value a non-throwing getItem would happily return, so this
    // test fails loudly (theme would come back 'light') if the throw below
    // isn't actually reaching the code path readStoredTheme() calls.
    localStorage.setItem(THEME_STORAGE_KEY, 'light');
    // Stub whatever object actually serves reads: real Storage instances
    // route through the shared prototype, but the in-memory polyfill used
    // when Node's experimental webstorage stub is active (see
    // setupVitest.ts) is a plain object with getItem as an own property,
    // so the prototype must be bypassed for the stub to take effect there.
    const target = localStorage instanceof Storage ? Storage.prototype : localStorage;
    const original = target.getItem;
    const throwingGetItem = vi.fn(() => {
      throw new Error('private mode');
    });
    target.getItem = throwingGetItem;
    try {
      const { result } = renderHook(() => useTheme(), { wrapper });
      expect(result.current.theme).toBe('dark');
      expect(throwingGetItem).toHaveBeenCalled();
    } finally {
      target.getItem = original;
    }
  });

  it('toggle flips the theme and persists it', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });
    act(() => result.current.toggle());
    expect(result.current.theme).toBe('light');
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');
    act(() => result.current.toggle());
    expect(result.current.theme).toBe('dark');
  });

  it('throws outside a ThemeProvider so misuse is loud', () => {
    expect(() => renderHook(() => useTheme())).toThrow(/ThemeProvider/);
  });
});
