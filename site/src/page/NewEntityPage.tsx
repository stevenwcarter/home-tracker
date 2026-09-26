import { useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { EntityForm } from 'components/EntityForm';
import { PageSkeleton } from 'components/PageSkeleton';
import { useCreateEntity } from 'hooks/useEntityMutations';
import { useEntityTypes } from 'hooks/useEntityTypes';
import { itemType } from 'types/builtIns';
import { EntityInput } from 'types/entity';
import { entityPath } from 'utils/entityPath';

/**
 * `/new` (top level) and `/locations/:id/new` (inside that location). An
 * optional `?type=<entity type id>` preselects the type, which is how "Add
 * location" differs from "Add item"; without it the Item type is preselected.
 * On save it opens the new entity's page.
 */
export const NewEntityPage = () => {
  const { id: parentId } = useParams();
  const [searchParams] = useSearchParams();
  const typeParam = searchParams.get('type');
  const { entityTypes, loading: typesLoading } = useEntityTypes();
  const typeId = typeParam ?? itemType(entityTypes)?.id;
  const { create, loading } = useCreateEntity();
  const navigate = useNavigate();

  const isLocation = entityTypes.find((type) => type.id === typeId)?.isLocation ?? false;

  const submit = async (input: EntityInput) => {
    const created = await create(input);
    // On failure the hook has toasted; the form keeps what was typed.
    if (created) navigate(entityPath(created));
  };

  // The form reads its default type once on mount, so wait for the types to pick Item.
  if (typeParam === null && typesLoading && entityTypes.length === 0) {
    return <PageSkeleton label="Loading form" />;
  }

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
