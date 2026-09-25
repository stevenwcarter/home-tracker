/// <reference types="node" />
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect } from 'vitest';

// Any raw Tailwind palette colour class (bg-zinc-900, text-red-500, ...).
// Components must use only the semantic classes derived from tokens.css
// (bg-surface, text-muted, ...) so a theme is exactly one block of
// variables; a raw palette class silently breaks the second theme.
const RAW_PALETTE_CLASS =
  /\b(bg|text|border|ring|fill|stroke|from|to|via|outline|decoration|accent|caret|divide|placeholder|shadow)-(slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|white|black)(-\d{2,3})?\b/;

const SRC_ROOT = path.resolve(__dirname, '../..');
const TOKENS_CSS_PATH = path.resolve(SRC_ROOT, 'theme/tokens.css');
const INDEX_HTML_PATH = path.resolve(SRC_ROOT, '../index.html');

/** Recursively lists files under `dir` matching `extension`. */
function listFiles(dir: string, extension: string): string[] {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  return entries.flatMap((entry) => {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) return listFiles(fullPath, extension);
    return entry.isFile() && entry.name.endsWith(extension) ? [fullPath] : [];
  });
}

function assertNoRawPaletteClasses(filePath: string, contents: string): void {
  if (!contents.includes('className')) return;
  const match = RAW_PALETTE_CLASS.exec(contents);
  if (match) {
    throw new Error(
      `${path.relative(SRC_ROOT, filePath)} uses a raw Tailwind palette colour class ` +
        `("${match[0]}"); use a semantic token class from theme/tokens.css instead.`,
    );
  }
}

describe('semantic token invariant', () => {
  it('never uses a raw Tailwind palette colour class in a .tsx source file', () => {
    for (const filePath of listFiles(SRC_ROOT, '.tsx')) {
      assertNoRawPaletteClasses(filePath, fs.readFileSync(filePath, 'utf8'));
    }
  });

  it('never uses a raw Tailwind palette colour class in index.html', () => {
    assertNoRawPaletteClasses(INDEX_HTML_PATH, fs.readFileSync(INDEX_HTML_PATH, 'utf8'));
  });

  it('declares the same --t-* variable names in the dark and light blocks', () => {
    const css = fs.readFileSync(TOKENS_CSS_PATH, 'utf8');
    const darkBlock = extractBlock(css, /:root,\s*\n:root\[data-theme=['"]dark['"]\]\s*\{/);
    const lightBlock = extractBlock(css, /:root\[data-theme=['"]light['"]\]\s*\{/);

    const darkNames = extractTokenNames(darkBlock);
    const lightNames = extractTokenNames(lightBlock);

    expect(darkNames.size).toBeGreaterThan(0);
    expect(lightNames).toEqual(darkNames);
  });

  it('maps every --t-* token to a --color-* entry in the @theme inline block', () => {
    const css = fs.readFileSync(TOKENS_CSS_PATH, 'utf8');
    const darkBlock = extractBlock(css, /:root,\s*\n:root\[data-theme=['"]dark['"]\]\s*\{/);
    const themeBlock = extractBlock(css, /@theme inline\s*\{/);

    const tokenNames = extractTokenNames(darkBlock);
    const referencedInTheme = new Set(
      [...themeBlock.matchAll(/var\((--t-[a-z0-9-]+)\)/g)].map((m) => m[1]),
    );

    for (const name of tokenNames) {
      expect(referencedInTheme.has(name), `${name} is not referenced by @theme inline`).toBe(true);
    }
  });
});

/** Extracts the body of the first `{ ... }` block whose opening matches `startPattern`. */
function extractBlock(css: string, startPattern: RegExp): string {
  const match = startPattern.exec(css);
  if (!match) throw new Error(`could not find a block matching ${startPattern}`);
  const bodyStart = match.index + match[0].length;
  const bodyEnd = css.indexOf('}', bodyStart);
  if (bodyEnd === -1) throw new Error(`unterminated block for ${startPattern}`);
  return css.slice(bodyStart, bodyEnd);
}

/** Extracts every `--t-*` custom property name declared in a CSS block. */
function extractTokenNames(block: string): Set<string> {
  return new Set([...block.matchAll(/(--t-[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
}
