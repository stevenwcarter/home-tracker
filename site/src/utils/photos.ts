import { AttachmentRef } from 'types/entity';

/**
 * Whether an attachment belongs in the photo gallery: a photo the server can
 * thumbnail. A photo it cannot (say `image/heic`) has no `thumbnailUrl` and is
 * listed as a plain file instead.
 */
export const isGalleryPhoto = (attachment: AttachmentRef): boolean =>
  attachment.kind === 'PHOTO' && attachment.thumbnailUrl !== null;
