import { useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { EntityForm } from 'components/EntityForm';
import { useCreateEntity } from 'hooks/useEntityMutations';
import { useEntityTypes } from 'hooks/useEntityTypes';
import { EntityInput } from 'types/entity';
import { entityPath } from 'utils/entityPath';

/**
 * `/new` (top level) and `/locations/:id/new` (inside that location). An
 * optional `?type=<entity type id>` preselects the type, which is how "Add
 * location" differs from "Add item". On save it opens the new entity's page.
 */
export const NewEntityPage = () => {
  const { id: parentId } = useParams();
  const [searchParams] = useSearchParams();
  const typeId = searchParams.get('type') ?? undefined;
  const { entityTypes } = useEntityTypes();
  const { create, loading } = useCreateEntity();
  const navigate = useNavigate();

  const isLocation = entityTypes.find((type) => type.id === typeId)?.isLocation ?? false;

  const submit = async (input: EntityInput) => {
    const created = await create(input);
    // On failure the hook has toasted; the form keeps what was typed.
    if (created) navigate(entityPath(created));
  };

  return (
    <section>
      <h1>{isLocation ? 'New location' : 'New item'}</h1>
      <div className="mt-6">
        <EntityForm
          mode="create"
          defaultParentId={parentId}
          defaultTypeId={typeId}
          onSubmit={submit}
          submitting={loading}
        />
      </div>
    </section>
  );
};

export default NewEntityPage;
