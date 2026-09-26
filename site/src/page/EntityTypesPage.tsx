import { FormEvent, useState } from 'react';
import {
  PRIMARY_ACTION,
  ROW_ACTION,
  ROW_DANGER_ACTION,
  SECONDARY_ACTION,
} from 'components/buttonStyles';
import { ConfirmDialog } from 'components/ConfirmDialog';
import { CheckboxField, FormField, INPUT_CLASS } from 'components/FormField';
import { Section } from 'components/Section';
import {
  useCreateEntityType,
  useDeleteEntityType,
  useUpdateEntityType,
} from 'hooks/useEntityTypeMutations';
import { useEntityTypes } from 'hooks/useEntityTypes';
import { ITEM_TYPE_ID, LOCATION_TYPE_ID } from 'types/builtIns';
import { EntityTypeDetail, EntityTypeInput } from 'types/entity';
import { plural } from 'utils/plural';

/** The seeded Location and Item types, which the server never deletes. */
const BUILT_IN_TYPE_IDS = new Set([LOCATION_TYPE_ID, ITEM_TYPE_ID]);

/** Why a type cannot be deleted, or null when it can. */
const deleteBlocker = (type: EntityTypeDetail): string | null => {
  if (BUILT_IN_TYPE_IDS.has(type.id)) return 'Built-in types cannot be deleted';
  if (type.entityCount > 0) return `In use by ${plural(type.entityCount, 'entity', 'entities')}`;
  return null;
};

interface Draft {
  name: string;
  description: string;
  isLocation: boolean;
}

const BLANK: Draft = { name: '', description: '', isLocation: false };

const draftOf = (type: EntityTypeDetail): Draft => ({
  name: type.name,
  description: type.description ?? '',
  isLocation: type.isLocation,
});

/** An update replaces every field, so the icon (not editable here) goes back as it came. */
const inputOf = (draft: Draft, icon: string | null): EntityTypeInput => ({
  name: draft.name.trim(),
  description: draft.description.trim() || null,
  icon,
  isLocation: draft.isLocation,
});

/**
 * The type fields as a form, used both for "New type" and for editing a row
 * in place. `onSubmit` resolves true on success; a create form then clears.
 */
const TypeForm = ({
  label,
  initial,
  submitLabel,
  busy,
  onSubmit,
  onCancel,
}: {
  label: string;
  initial: Draft;
  submitLabel: string;
  busy: boolean;
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

  return (
    <form
      aria-label={label}
      onSubmit={submit}
      noValidate
      className="grid gap-3 sm:grid-cols-[1fr_2fr_auto_auto] sm:items-end"
    >
      <FormField label="Name">
        {(control) => (
          <input
            {...control}
            type="text"
            value={draft.name}
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
            className={INPUT_CLASS}
          />
        )}
      </FormField>
      <FormField label="Description">
        {(control) => (
          <input
            {...control}
            type="text"
            value={draft.description}
            onChange={(event) => setDraft({ ...draft, description: event.target.value })}
            className={INPUT_CLASS}
          />
        )}
      </FormField>
      <div className="pb-1.5">
        <CheckboxField
          label="Holds other things (location)"
          checked={draft.isLocation}
          onChange={(isLocation) => setDraft({ ...draft, isLocation })}
        />
      </div>
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

const TypeRow = ({
  type,
  onEdit,
  onDelete,
}: {
  type: EntityTypeDetail;
  onEdit: () => void;
  onDelete: () => void;
}) => {
  const blocker = deleteBlocker(type);
  return (
    <tr className="border-t border-border">
      <td className="px-3 py-2">
        <span className="font-medium text-text">{type.name}</span>
        {type.description && <div className="text-sm text-muted">{type.description}</div>}
      </td>
      <td className="px-3 py-2 text-muted">{type.isLocation ? 'Location' : 'Item'}</td>
      <td className="px-3 py-2 text-right tabular-nums">{type.entityCount}</td>
      <td className="px-3 py-2 text-right whitespace-nowrap">
        <button
          type="button"
          onClick={onEdit}
          aria-label={`Edit ${type.name}`}
          className={ROW_ACTION}
        >
          Edit
        </button>
        <button
          type="button"
          onClick={onDelete}
          disabled={blocker !== null}
          title={blocker ?? undefined}
          aria-label={`Delete ${type.name}`}
          className={ROW_DANGER_ACTION}
        >
          Delete
        </button>
      </td>
    </tr>
  );
};

/** `/types`: every entity type with its usage, created, edited in place and deleted here. */
export const EntityTypesPage = () => {
  const { entityTypes, loading, error } = useEntityTypes();
  const { create, loading: creating } = useCreateEntityType();
  const { update, loading: updating } = useUpdateEntityType();
  const { remove, loading: removing } = useDeleteEntityType();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [doomed, setDoomed] = useState<EntityTypeDetail | null>(null);

  const deleteDoomed = async () => {
    if (!doomed) return;
    // On failure the hook has toasted; either way the dialog closes.
    await remove(doomed.id);
    setDoomed(null);
  };

  return (
    <section>
      <h1>Types</h1>
      <p className="mt-2 text-muted">
        Every item and location has a type. Location types can hold other things.
      </p>

      <Section title="New type">
        <div className="rounded-lg border border-border bg-surface p-4">
          <TypeForm
            label="New type"
            initial={BLANK}
            submitLabel="Add type"
            busy={creating}
            onSubmit={async (draft) => (await create(inputOf(draft, null))) !== null}
          />
        </div>
      </Section>

      <Section title="All types">
        {loading && entityTypes.length === 0 && <p className="text-muted">Loading types…</p>}
        {error && entityTypes.length === 0 && (
          <p className="text-danger">Could not load the types.</p>
        )}
        {entityTypes.length > 0 && (
          <div className="overflow-x-auto rounded-lg border border-border bg-surface">
            <table className="w-full text-sm">
              <thead className="text-left text-muted">
                <tr>
                  <th className="px-3 py-2 font-medium">Name</th>
                  <th className="px-3 py-2 font-medium">Kind</th>
                  <th className="px-3 py-2 text-right font-medium">Entities</th>
                  <th className="px-3 py-2">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {entityTypes.map((type) =>
                  type.id === editingId ? (
                    <tr key={type.id} className="border-t border-border">
                      <td colSpan={4} className="px-3 py-3">
                        <TypeForm
                          label={`Edit ${type.name}`}
                          initial={draftOf(type)}
                          submitLabel="Save"
                          busy={updating}
                          onSubmit={async (draft) => {
                            const saved = await update(type.id, inputOf(draft, type.icon));
                            if (saved) setEditingId(null);
                            return saved !== null;
                          }}
                          onCancel={() => setEditingId(null)}
                        />
                      </td>
                    </tr>
                  ) : (
                    <TypeRow
                      key={type.id}
                      type={type}
                      onEdit={() => setEditingId(type.id)}
                      onDelete={() => setDoomed(type)}
                    />
                  ),
                )}
              </tbody>
            </table>
          </div>
        )}
      </Section>

      <ConfirmDialog
        open={doomed !== null}
        title={`Delete ${doomed?.name ?? ''}?`}
        body="This cannot be undone."
        confirmLabel="Delete type"
        onConfirm={deleteDoomed}
        onCancel={() => setDoomed(null)}
        busy={removing}
      />
    </section>
  );
};

export default EntityTypesPage;
