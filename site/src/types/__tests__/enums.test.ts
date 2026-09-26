import { describe, it, expect } from 'vitest';
import type { AttachmentKind, FieldKind } from '../entity';

// Hand-kept mirrors of the backend enums (`AttachmentKind`, `FieldKind` in
// src/kinds.rs, upper-cased by GraphQL). `satisfies` rejects a value the TS
// union lacks; `Covers` fails to compile when a union member is missing from
// the list; the length and value assertions pin the list to the backend.
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

describe('enum unions', () => {
  it('AttachmentKind matches the backend names', () => {
    expect(attachmentKindsComplete).toBe(true);
    expect(ATTACHMENT_KINDS).toHaveLength(5);
    expect([...ATTACHMENT_KINDS].sort()).toEqual(
      ['ATTACHMENT', 'MANUAL', 'PHOTO', 'RECEIPT', 'WARRANTY'].sort(),
    );
  });

  it('FieldKind matches the backend names', () => {
    expect(fieldKindsComplete).toBe(true);
    expect(FIELD_KINDS).toHaveLength(4);
    expect([...FIELD_KINDS].sort()).toEqual(['BOOLEAN', 'NUMBER', 'TEXT', 'TIME']);
  });
});
