import { KeyboardEvent, useEffect, useId, useRef, useState } from 'react';
import { useTags } from 'hooks/useTags';
import { useCreateTag } from 'hooks/useTagMutations';
import { CheckboxField, INPUT_CLASS } from './FormField';

interface TagPickerProps {
  /** The selected tag ids. */
  value: string[];
  onChange: (ids: string[]) => void;
}

/** A checkbox per tag, plus an inline "New tag" box that creates one and selects it. */
export const TagPicker = ({ value, onChange }: TagPickerProps) => {
  const { tags } = useTags();
  const { create, loading: creating } = useCreateTag();
  const [newName, setNewName] = useState('');
  const newTagId = useId();
  // The selection as of the latest render, for when a create resolves after
  // the user has toggled other tags meanwhile.
  const valueRef = useRef(value);
  useEffect(() => {
    valueRef.current = value;
  }, [value]);

  const select = (id: string) => {
    if (!valueRef.current.includes(id)) onChange([...valueRef.current, id]);
  };

  const toggle = (id: string, checked: boolean) =>
    onChange(checked ? [...value, id] : value.filter((tagId) => tagId !== id));

  const trimmed = newName.trim();

  const addTag = async () => {
    if (!trimmed || creating) return;
    const existing = tags.find((tag) => tag.name.toLowerCase() === trimmed.toLowerCase());
    if (existing) {
      select(existing.id);
      setNewName('');
      return;
    }
    const tag = await create({
      name: trimmed,
      description: null,
      color: null,
      icon: null,
      parentId: null,
    });
    // On failure the hook has already toasted; keep the typed name to retry.
    if (!tag) return;
    select(tag.id);
    setNewName('');
  };

  // Enter adds the tag rather than submitting the form this picker sits in.
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    void addTag();
  };

  return (
    <fieldset>
      <legend className="mb-2 text-sm font-medium text-muted">Tags</legend>
      {tags.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-x-4 gap-y-2">
          {tags.map((tag) => (
            <CheckboxField
              key={tag.id}
              label={tag.name}
              checked={value.includes(tag.id)}
              onChange={(checked) => toggle(tag.id, checked)}
            />
          ))}
        </div>
      )}
      <label htmlFor={newTagId} className="mb-1 block text-sm font-medium text-muted">
        New tag
      </label>
      <div className="flex gap-2">
        <input
          id={newTagId}
          type="text"
          value={newName}
          onChange={(event) => setNewName(event.target.value)}
          onKeyDown={onKeyDown}
          className={INPUT_CLASS}
        />
        <button
          type="button"
          onClick={() => void addTag()}
          disabled={!trimmed || creating}
          className="shrink-0 rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-raised disabled:opacity-50"
        >
          Add tag
        </button>
      </div>
    </fieldset>
  );
};
