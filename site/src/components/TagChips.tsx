import { TagRef } from 'types/entity';

export const TagChips = ({ tags }: { tags: TagRef[] }) =>
  tags.length === 0 ? null : (
    <ul aria-label="Tags" className="flex flex-wrap gap-2">
      {tags.map((tag) => (
        <li
          key={tag.id}
          className="flex items-center gap-1.5 rounded-full border border-border bg-surface-raised px-3 py-0.5 text-sm text-text"
        >
          {tag.color && (
            <span
              aria-hidden="true"
              className="inline-block h-2 w-2 rounded-full"
              style={{ backgroundColor: tag.color }}
            />
          )}
          {tag.name}
        </li>
      ))}
    </ul>
  );
