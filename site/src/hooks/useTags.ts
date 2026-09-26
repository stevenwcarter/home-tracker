import { useMemo } from 'react';
import { useQuery } from '@apollo/client/react';
import { TagDetail } from 'types/entity';
import { GET_TAGS } from './queries';
import { useErrorToast } from './useErrorToast';

/** A tag as `TagFields` selects it: the parent as an object, not an id. */
export type TagWire = Omit<TagDetail, 'parentId'> & { parent: { id: string } | null };

interface TagsResponse {
  tags: TagWire[];
}

/** Flattens the wire `parent { id }` to `parentId`. */
export const toTagDetail = ({ parent, ...tag }: TagWire): TagDetail => ({
  ...tag,
  parentId: parent?.id ?? null,
});

const NO_TAGS: TagWire[] = [];

export const useTags = () => {
  const { data, loading, error } = useQuery<TagsResponse>(GET_TAGS);
  useErrorToast(error, 'Error loading tags');
  const wire = data?.tags ?? NO_TAGS;
  const tags = useMemo(() => wire.map(toTagDetail), [wire]);
  return { tags, loading, error };
};
