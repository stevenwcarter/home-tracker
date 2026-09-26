import { useNavigate, useParams } from 'react-router-dom';
import { EntityForm } from 'components/EntityForm';
import { PageSkeleton } from 'components/PageSkeleton';
import { useEntity } from 'hooks/useEntity';
import { useUpdateEntity } from 'hooks/useEntityMutations';
import { NotFound } from 'page/NotFound';
import { EntityInput } from 'types/entity';
import { entityPath } from 'utils/entityPath';

/** `/items/:id/edit` and `/locations/:id/edit`: the shared form; on save, back to the entity's page. */
export const EditEntityPage = () => {
  const { id = '' } = useParams();
  const { entity, loading, notFound } = useEntity(id);
  const { update, loading: saving } = useUpdateEntity();
  const navigate = useNavigate();

  // A refetch after a save keeps the loaded entity on screen; only a first load shows the skeleton.
  if (loading && !entity) return <PageSkeleton label="Loading" />;
  if (notFound) return <NotFound />;
  if (!entity) return <p className="text-danger">Could not load this entry.</p>;

  const submit = async (input: EntityInput) => {
    const updated = await update(entity, input);
    // On failure the hook has toasted; the form keeps the edits.
    if (updated) navigate(entityPath(updated));
  };

  return (
    <section>
      <h1>Edit {entity.name}</h1>
      <div className="mt-6">
        {/* The form reads `initial` once on mount, so a different entity needs a fresh form. */}
        <EntityForm
          key={entity.id}
          mode="edit"
          initial={entity}
          onSubmit={submit}
          submitting={saving}
        />
      </div>
    </section>
  );
};

export default EditEntityPage;
