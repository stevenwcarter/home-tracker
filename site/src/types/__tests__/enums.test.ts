/// <reference types="node" />
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect } from 'vitest';
import type { AttachmentKind, FieldKind } from '../entity';

// The TS unions mirror the backend enums (`AttachmentKind`, `FieldKind` in
// src/kinds.rs), which GraphQL exposes with upper-cased variant names. These
// lists are read by the test below against kinds.rs itself, so a variant
// added on either side fails here. `satisfies` rejects a value the TS union
// lacks; `Covers` fails to compile when a union member is missing from a list.
const ATTACHMENT_KINDS = [
  'PHOTO',
  'MANUAL',
  'WARRANTY',
  'ATTACHMENT',
  'RECEIPT',
] as const satisfies readonly AttachmentKind[];
const FIELD_KINDS = ['TEXT', 'NUMBER', 'BOOLEAN', 'TIME'] as const satisfies readonly FieldKind[];

/** `true` only when every member of `Union` appears in `List`. */
type Covers<Union, List> = [Exclude<Union, List>] extends [never] ? true : false;
const attachmentKindsComplete: Covers<AttachmentKind, (typeof ATTACHMENT_KINDS)[number]> = true;
const fieldKindsComplete: Covers<FieldKind, (typeof FIELD_KINDS)[number]> = true;

// site/src/types/__tests__ -> the repository root.
const KINDS_RS = path.resolve(__dirname, '../../../../src/kinds.rs');

/**
 * The GraphQL names of `name`'s variants, read from its `text_enum!` block
 * (`Variant => "text",` lines). GraphQL spells a variant in SCREAMING_SNAKE
 * case (`Photo` -> `PHOTO`, `LifetimeWarranty` -> `LIFETIME_WARRANTY`).
 */
const rustVariants = (name: string): string[] => {
  const source = fs.readFileSync(KINDS_RS, 'utf8');
  const block = new RegExp(`text_enum!\\(\\s*${name}\\s*,[^{]*\\{([^}]*)\\}`).exec(source);
  if (!block) throw new Error(`no text_enum! block for ${name} in ${KINDS_RS}`);
  return [...block[1].matchAll(/^\s*([A-Z][A-Za-z0-9]*)\s*=>\s*"[^"]*"\s*,?\s*$/gm)].map(
    ([, variant]) => variant.replace(/(?<=[a-z0-9])(?=[A-Z])/g, '_').toUpperCase(),
  );
};

describe('enum unions', () => {
  it('AttachmentKind lists exactly the backend variants, in order', () => {
    expect(attachmentKindsComplete).toBe(true);
    const backend = rustVariants('AttachmentKind');
    expect(backend.length).toBeGreaterThan(0);
    expect([...ATTACHMENT_KINDS]).toEqual(backend);
  });

  it('FieldKind lists exactly the backend variants, in order', () => {
    expect(fieldKindsComplete).toBe(true);
    const backend = rustVariants('FieldKind');
    expect(backend.length).toBeGreaterThan(0);
    expect([...FIELD_KINDS]).toEqual(backend);
  });
});
