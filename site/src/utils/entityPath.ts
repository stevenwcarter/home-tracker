/** The page for an entity: locations and items live under different routes. */
export const entityPath = ({ id, isLocation }: { id: string; isLocation: boolean }) =>
  isLocation ? `/locations/${id}` : `/items/${id}`;
