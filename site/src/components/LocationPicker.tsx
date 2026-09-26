import { useLocations } from 'hooks/useLocations';
import { LocationNode } from 'utils/locationTree';
import { FormField, INPUT_CLASS } from './FormField';

/** Three non-breaking spaces per level: plain spaces collapse inside an `<option>`. */
const INDENT = '   ';

interface PickerOption {
  id: string;
  name: string;
  depth: number;
}

/**
 * The tree in display order (depth first, names sorted as the tree sorts
 * them), leaving out `excludedId` together with everything beneath it.
 */
function flatten(nodes: LocationNode[], excludedId: string | undefined, depth = 0): PickerOption[] {
  return nodes.flatMap(({ location, children }) =>
    location.id === excludedId
      ? []
      : [
          { id: location.id, name: location.name, depth },
          ...flatten(children, excludedId, depth + 1),
        ],
  );
}

interface LocationPickerProps {
  /** The selected location id, or null for top level. */
  value: string | null;
  onChange: (id: string | null) => void;
  /**
   * An entity that is being moved: it and its descendants are never offered,
   * since parenting it under one of them would make a cycle.
   */
  excludeSubtreeOf?: string;
  /** Offers `None (top level)`, which reports null. */
  allowNone?: boolean;
  label?: string;
}

/** A `<select>` over every location, indented by depth in the tree. */
export const LocationPicker = ({
  value,
  onChange,
  excludeSubtreeOf,
  allowNone = false,
  label = 'Parent location',
}: LocationPickerProps) => {
  const { tree, loading } = useLocations();
  const options = flatten(tree, excludeSubtreeOf);
  // An item may sit inside another item, which this list of locations doesn't
  // hold; keep that parent selectable so an untouched form sends it back.
  const unlisted =
    value !== null && !loading && !options.some((option) => option.id === value) ? value : null;

  return (
    <FormField label={label}>
      {(control) => (
        <select
          {...control}
          value={value ?? ''}
          onChange={(event) => onChange(event.target.value === '' ? null : event.target.value)}
          className={INPUT_CLASS}
        >
          {allowNone && <option value="">None (top level)</option>}
          {unlisted !== null && <option value={unlisted}>Current parent (not a location)</option>}
          {options.map(({ id, name, depth }) => (
            <option key={id} value={id}>
              {INDENT.repeat(depth)}
              {name}
            </option>
          ))}
        </select>
      )}
    </FormField>
  );
};
