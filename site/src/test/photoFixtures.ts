import { AttachmentRef } from 'types/entity';

/** A photo attachment as `GetEntity` returns it, with thumbnails. */
export const photoRef = (
  overrides: Partial<AttachmentRef> & Pick<AttachmentRef, 'id'>,
): AttachmentRef =>
  ({
    __typename: 'Attachment',
    kind: 'PHOTO',
    primary: false,
    title: `${overrides.id}.jpg`,
    mimeType: 'image/jpeg',
    url: `/attachments/${overrides.id}?v=abc`,
    thumbnailUrl: `/attachments/${overrides.id}/thumb/500?v=abc`,
    ...overrides,
  }) as AttachmentRef;
