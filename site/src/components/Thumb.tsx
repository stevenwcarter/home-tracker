import clsx from 'clsx';
import { ThumbSize, thumbUrlAt } from 'utils/thumbUrl';

interface ThumbProps {
  attachment: { thumbnailUrl: string | null; title?: string };
  size: ThumbSize;
  className?: string;
}

export const Thumb = ({ attachment, size, className }: ThumbProps) =>
  attachment.thumbnailUrl ? (
    <img
      src={thumbUrlAt(attachment.thumbnailUrl, size)}
      alt={attachment.title ?? ''}
      loading="lazy"
      className={clsx('rounded-md bg-surface-raised object-cover', className)}
    />
  ) : (
    <div
      className={clsx(
        'flex items-center justify-center rounded-md border border-border bg-surface-raised',
        className,
      )}
    >
      <span className="text-xs text-muted">No photo</span>
    </div>
  );
