import { FormEvent, useState } from 'react';
import {
  PRIMARY_ACTION,
  ROW_ACTION,
  ROW_DANGER_ACTION,
  SECONDARY_ACTION,
} from 'components/buttonStyles';
import { ConfirmDialog } from 'components/ConfirmDialog';
import { FormField, INPUT_CLASS } from 'components/FormField';
import { Section } from 'components/Section';
import { useCreateTag, useDeleteTag, useUpdateTag } from 'hooks/useTagMutations';
import { useTags } from 'hooks/useTags';
import { TagDetail, TagInput } from 'types/entity';
import { plural } from 'utils/plural';

interface Draft {
  name: string;
  color: string;
  parentId: string;
  description: string;
}

const BLANK: Draft = { name: '', color: '', parentId: '', description: '' };

const draftOf = (tag: TagDetail): Draft => ({
  name: tag.name,
  color: tag.color ?? '',
  parentId: tag.parentId ?? '',
  description: tag.description ?? '',
});

/** An update replaces every field, so the icon (not editable here) goes back as it came. */
const inputOf = (draft: Draft, icon: string | null): TagInput => ({
  name: draft.name.trim(),
  description: draft.description.trim() || null,
  color: draft.color.trim() || null,
  icon,
  parentId: draft.parentId || null,
});

/** `id` and every tag beneath it: none of them may become its parent (the server refuses cycles too). */
const subtreeOf = (id: string, tags: TagDetail[]): Set<string> => {
  const subtree = new Set([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const tag of tags) {
      if (tag.parentId && subtree.has(tag.parentId) && !subtree.has(tag.id)) {
        subtree.add(tag.id);
        grew = true;
      }
    }
  }
  return subtree;
};

/** A colour chip; the colour is the user's own data, hence the inline style. */
const Swatch = ({ tag }: { tag: TagDetail }) => (
  <span
    aria-hidden="true"
    data-testid={`swatch-${tag.id}`}
    style={tag.color ? { background: tag.color } : undefined}
    className="inline-block h-3 w-3 shrink-0 rounded-full border border-border"
  />
);

/**
 * The tag fields as a form, used both for "New tag" and for editing a row in
 * place. `onSubmit` resolves true on success; a create form then clears.
 */
const TagForm = ({
  label,
  initial,
  submitLabel,
  busy,
  parents,
  onSubmit,
  onCancel,
}: {
  label: string;
  initial: Draft;
  submitLabel: string;
  busy: boolean;
  /** The tags this one may sit under. */
  parents: TagDetail[];
  onSubmit: (draft: Draft) => Promise<boolean>;
  onCancel?: () => void;
}) => {
  const [draft, setDraft] = useState(initial);
  const blank = draft.name.trim() === '';

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (blank || busy) return;
    if ((await onSubmit(draft)) && !onCancel) setDraft(BLANK);
  };

  const text = (fieldLabel: string, key: 'name' | 'color' | 'description', placeholder = '') => (
    <FormField label={fieldLabel}>
      {(control) => (
        <input
          {...control}
          type="text"
          placeholder={placeholder}
          value={draft[key]}
          onChange={(event) => setDraft({ ...draft, [key]: event.target.value })}
          className={INPUT_CLASS}
        />
      )}
    </FormField>
  );

  return (
    <form
      aria-label={label}
      onSubmit={submit}
      noValidate
      className="grid gap-3 sm:grid-cols-2 lg:grid-cols-[1fr_8rem_1fr_1fr_auto] lg:items-end"
    >
      {text('Name', 'name')}
      {text('Colour', 'color', '#4a90d9')}
      <FormField label="Parent tag">
        {(control) => (
          <select
            {...control}
            value={draft.parentId}
            onChange={(event) => setDraft({ ...draft, parentId: event.target.value })}
            className={INPUT_CLASS}
          >
            <option value="">None</option>
            {parents.map((tag) => (
              <option key={tag.id} value={tag.id}>
                {tag.name}
              </option>
            ))}
          </select>
        )}
      </FormField>
      {text('Description', 'description')}
      <div className="flex gap-2">
        {onCancel && (
          <button type="button" onClick={onCancel} className={SECONDARY_ACTION}>
            Cancel
          </button>
        )}
        <button type="submit" disabled={blank || busy} className={PRIMARY_ACTION}>
          {submitLabel}
        </button>
      </div>
    </form>
  );
};

const TagRow = ({
  tag,
  parentName,
  onEdit,
  onDelete,
}: {
  tag: TagDetail;
  parentName: string | null;
  onEdit: () => void;
  onDelete: () => void;
}) => (
  <tr className="border-t border-border">
    <td className="px-3 py-2">
      <span className="flex items-center gap-2">
        <Swatch tag={tag} />
        <span className="font-medium text-text">{tag.name}</span>
      </span>
      {tag.description && <div className="text-sm text-muted">{tag.description}</div>}
    </td>
    <td className="px-3 py-2 text-muted">{parentName}</td>
    <td className="px-3 py-2 text-right tabular-nums">{tag.entityCount}</td>
    <td className="px-3 py-2 text-right whitespace-nowrap">
      <button type="button" onClick={onEdit} aria-label={`Edit ${tag.name}`} className={ROW_ACTION}>
        Edit
      </button>
      <button
        type="button"
        onClick={onDelete}
        aria-label={`Delete ${tag.name}`}
        className={ROW_DANGER_ACTION}
      >
        Delete
      </button>
    </td>
  </tr>
);

/** `/tags`: every tag with its colour, parent and usage, created, edited in place and deleted here. */
export const TagsPage = () => {
  const { tags, loading, error } = useTags();
  const { create, loading: creating } = useCreateTag();
  const { update, loading: updating } = useUpdateTag();
  const { remove, loading: removing } = useDeleteTag();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [doomed, setDoomed] = useState<TagDetail | null>(null);

  const nameOf = (id: string | null) => tags.find((tag) => tag.id === id)?.name ?? null;

  const deleteDoomed = async () => {
    if (!doomed) return;
    // On failure the hook has toasted; either way the dialog closes.
    await remove(doomed.id);
    setDoomed(null);
  };

  return (
    <section>
      <h1>Tags</h1>

      <Section title="New tag">
        <div className="rounded-lg border border-border bg-surface p-4">
          <TagForm
            label="New tag"
            initial={BLANK}
            submitLabel="Add tag"
            busy={creating}
            parents={tags}
            onSubmit={async (draft) => (await create(inputOf(draft, null))) !== null}
          />
        </div>
      </Section>

      <Section title="All tags">
        {loading && tags.length === 0 && <p className="text-muted">Loading tags…</p>}
        {error && tags.length === 0 && <p className="text-danger">Could not load the tags.</p>}
        {!loading && !error && tags.length === 0 && <p className="text-muted">No tags yet.</p>}
        {tags.length > 0 && (
          <div className="overflow-x-auto rounded-lg border border-border bg-surface">
            <table className="w-full text-sm">
              <thead className="text-left text-muted">
                <tr>
                  <th className="px-3 py-2 font-medium">Name</th>
                  <th className="px-3 py-2 font-medium">Parent</th>
                  <th className="px-3 py-2 text-right font-medium">Entities</th>
                  <th className="px-3 py-2">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {tags.map((tag) => {
                  if (tag.id !== editingId) {
                    return (
                      <TagRow
                        key={tag.id}
                        tag={tag}
                        parentName={nameOf(tag.parentId)}
                        onEdit={() => setEditingId(tag.id)}
                        onDelete={() => setDoomed(tag)}
                      />
                    );
                  }
                  const excluded = subtreeOf(tag.id, tags);
                  return (
                    <tr key={tag.id} className="border-t border-border">
                      <td colSpan={4} className="px-3 py-3">
                        <TagForm
                          label={`Edit ${tag.name}`}
                          initial={draftOf(tag)}
                          submitLabel="Save"
                          busy={updating}
                          parents={tags.filter((candidate) => !excluded.has(candidate.id))}
                          onSubmit={async (draft) => {
                            const saved = await update(tag.id, inputOf(draft, tag.icon));
                            if (saved) setEditingId(null);
                            return saved !== null;
                          }}
                          onCancel={() => setEditingId(null)}
                        />
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Section>

      <ConfirmDialog
        open={doomed !== null}
        title={`Delete ${doomed?.name ?? ''}?`}
        body={
          doomed && doomed.entityCount > 0
            ? `This cannot be undone. It will be removed from ${plural(doomed.entityCount, 'entity', 'entities')}.`
            : 'This cannot be undone.'
        }
        confirmLabel="Delete tag"
        onConfirm={deleteDoomed}
        onCancel={() => setDoomed(null)}
        busy={removing}
      />
    </section>
  );
};

export default TagsPage;
