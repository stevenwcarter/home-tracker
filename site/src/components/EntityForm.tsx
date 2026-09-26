import { FormEvent, ReactNode, useId, useState } from 'react';
import { Link } from 'react-router-dom';
import { useEntityTypes } from 'hooks/useEntityTypes';
import { EntityDetail, EntityInput } from 'types/entity';
import { toEntityInput } from 'utils/entityInput';
import { formatMoneyInput, MoneyParseError, parseMoney } from 'utils/money';
import { CheckboxField, FormField, INPUT_CLASS } from './FormField';
import { LocationPicker } from './LocationPicker';
import { TagPicker } from './TagPicker';

type EntityFormProps = {
  onSubmit: (input: EntityInput) => void;
  /** Disables Save while the create or update is in flight. */
  submitting: boolean;
  /** Where Cancel leads; defaults to the entity's page, or the default parent's (else home) on create. */
  cancelTo?: string;
  /** The submit button's text; "Save" unless given. */
  submitLabel?: string;
  /** Shown in place of the Cancel link (the AI review's Skip). */
  secondaryAction?: ReactNode;
} & (
  | {
      mode: 'create';
      initial?: undefined;
      /** A complete input to start from (an AI suggestion), instead of the defaults below. */
      initialInput?: EntityInput;
      defaultParentId?: string;
      defaultTypeId?: string;
    }
  | {
      mode: 'edit';
      initial: EntityDetail;
      initialInput?: undefined;
      defaultParentId?: undefined;
      defaultTypeId?: undefined;
    }
);

/** Every field at its default, for a new entity. */
const blankInput = (entityTypeId: string, parentId: string | null): EntityInput => ({
  name: '',
  description: null,
  entityTypeId,
  parentId,
  archived: false,
  quantity: 1,
  insured: false,
  serialNumber: null,
  modelNumber: null,
  manufacturer: null,
  notes: null,
  lifetimeWarranty: false,
  warrantyExpires: null,
  warrantyDetails: null,
  purchaseDate: null,
  purchaseFrom: null,
  purchasePriceCents: 0,
  soldDate: null,
  soldTo: null,
  soldPriceCents: 0,
  soldNotes: null,
  tagIds: [],
});

type MoneyKey = 'purchasePriceCents' | 'soldPriceCents';
type TextKey = {
  [K in keyof EntityInput]: EntityInput[K] extends string | null ? K : never;
}[keyof EntityInput];
type BooleanKey = {
  [K in keyof EntityInput]: EntityInput[K] extends boolean ? K : never;
}[keyof EntityInput];
type CollapsibleKey = 'purchase' | 'warranty' | 'sold';

/** The inline message for a money string that doesn't parse, else null. */
function moneyError(text: string): string | null {
  try {
    parseMoney(text);
    return null;
  } catch (error) {
    if (error instanceof MoneyParseError) return error.message;
    throw error;
  }
}

/** Quantity is a float in the API (imports carry values like 1.5); only negatives are refused. */
function parseQuantity(text: string): number | null {
  if (text.trim() === '') return null;
  const quantity = Number(text);
  return Number.isFinite(quantity) && quantity >= 0 ? quantity : null;
}

/** A blank control means "none": the API takes null, not an empty string. */
const optional = (text: string): string | null => (text === '' ? null : text);

const defaultCancelTo = (props: EntityFormProps): string => {
  if (props.mode === 'edit') {
    const { id, isLocation } = props.initial;
    return isLocation ? `/locations/${id}` : `/items/${id}`;
  }
  return props.defaultParentId ? `/locations/${props.defaultParentId}` : '/';
};

const Fieldset = ({ legend, children }: { legend: string; children: ReactNode }) => (
  <fieldset className="rounded-lg border border-border bg-surface p-4">
    <legend className="px-1 text-base font-semibold text-text">{legend}</legend>
    <div className="grid gap-4 sm:grid-cols-2">{children}</div>
  </fieldset>
);

/** A section whose body shows and hides behind its legend's toggle button. */
const CollapsibleFieldset = ({
  legend,
  open,
  onToggle,
  children,
}: {
  legend: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) => {
  const bodyId = useId();
  return (
    <fieldset className="rounded-lg border border-border bg-surface p-4">
      <legend className="px-1">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={onToggle}
          className="flex items-center gap-2 text-base font-semibold text-text hover:text-accent"
        >
          <span aria-hidden="true" className="w-3 text-muted">
            {open ? '▾' : '▸'}
          </span>
          {legend}
        </button>
      </legend>
      <div id={bodyId} hidden={!open} className="grid gap-4 sm:grid-cols-2">
        {children}
      </div>
    </fieldset>
  );
};

/**
 * The create and edit form for any entity (item or location). It always
 * submits a complete `EntityInput`: `updateEntity` replaces every field, so an
 * edit starts from `initial` and fields the user didn't touch go back as they
 * came.
 */
export const EntityForm = (props: EntityFormProps) => {
  const { mode, initial, onSubmit, submitting } = props;
  const { entityTypes } = useEntityTypes();

  const [input, setInput] = useState<EntityInput>(() =>
    initial
      ? toEntityInput(initial)
      : (props.initialInput ??
        blankInput(props.defaultTypeId ?? '', props.defaultParentId ?? null)),
  );
  // An edit shows every amount; a new entity leaves a zero amount blank.
  const [money, setMoney] = useState<Record<MoneyKey, string>>(() => {
    const shown = (cents: number) => (initial || cents !== 0 ? formatMoneyInput(cents) : '');
    return {
      purchasePriceCents: shown(input.purchasePriceCents),
      soldPriceCents: shown(input.soldPriceCents),
    };
  });
  const [quantityText, setQuantityText] = useState(() => String(input.quantity));
  const [nameTouched, setNameTouched] = useState(false);
  // Sections the user has opened or closed; the rest follow the type's default.
  const [toggled, setToggled] = useState<Partial<Record<CollapsibleKey, boolean>>>({});

  const set = <K extends keyof EntityInput>(key: K, value: EntityInput[K]) =>
    setInput((previous) => ({ ...previous, [key]: value }));

  const isLocation =
    entityTypes.find((type) => type.id === input.entityTypeId)?.isLocation ??
    initial?.entityType.isLocation ??
    false;
  const isOpen = (key: CollapsibleKey) => toggled[key] ?? !isLocation;
  const toggle = (key: CollapsibleKey) =>
    setToggled((previous) => ({ ...previous, [key]: !isOpen(key) }));

  const nameBlank = input.name.trim() === '';
  const nameError = nameTouched && nameBlank ? 'Enter a name' : null;
  const quantity = parseQuantity(quantityText);
  const quantityError = quantity === null ? 'Enter a number, 0 or more' : null;
  const moneyErrors: Record<MoneyKey, string | null> = {
    purchasePriceCents: moneyError(money.purchasePriceCents),
    soldPriceCents: moneyError(money.soldPriceCents),
  };
  const invalid =
    nameBlank ||
    input.entityTypeId === '' ||
    quantityError !== null ||
    Object.values(moneyErrors).some((error) => error !== null);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (invalid || submitting) return;
    onSubmit({
      ...input,
      name: input.name.trim(),
      quantity: quantity ?? input.quantity,
      purchasePriceCents: parseMoney(money.purchasePriceCents),
      soldPriceCents: parseMoney(money.soldPriceCents),
    });
  };

  const text = (label: string, key: TextKey, type: 'text' | 'date' = 'text') => (
    <FormField label={label}>
      {(control) => (
        <input
          {...control}
          type={type}
          value={input[key] ?? ''}
          onChange={(event) => set(key, optional(event.target.value))}
          className={INPUT_CLASS}
        />
      )}
    </FormField>
  );

  const longText = (label: string, key: TextKey) => (
    <FormField label={label} className="sm:col-span-2">
      {(control) => (
        <textarea
          {...control}
          rows={3}
          value={input[key] ?? ''}
          onChange={(event) => set(key, optional(event.target.value))}
          className={INPUT_CLASS}
        />
      )}
    </FormField>
  );

  const checkbox = (label: string, key: BooleanKey) => (
    <CheckboxField label={label} checked={input[key]} onChange={(checked) => set(key, checked)} />
  );

  const moneyField = (label: string, key: MoneyKey) => (
    <FormField label={label} error={moneyErrors[key]}>
      {(control) => (
        <input
          {...control}
          type="text"
          inputMode="decimal"
          placeholder="0.00"
          value={money[key]}
          onChange={(event) => {
            const value = event.target.value;
            setMoney((previous) => ({ ...previous, [key]: value }));
          }}
          className={INPUT_CLASS}
        />
      )}
    </FormField>
  );

  return (
    <form
      onSubmit={submit}
      aria-label={mode === 'edit' ? `Edit ${initial.name}` : 'New entity'}
      className="flex flex-col gap-6"
      noValidate
    >
      <Fieldset legend="Basics">
        <FormField label="Name" error={nameError}>
          {(control) => (
            <input
              {...control}
              type="text"
              required
              value={input.name}
              onChange={(event) => {
                setNameTouched(true);
                set('name', event.target.value);
              }}
              className={INPUT_CLASS}
            />
          )}
        </FormField>
        <FormField label="Type">
          {(control) => (
            <select
              {...control}
              required
              value={input.entityTypeId}
              onChange={(event) => set('entityTypeId', event.target.value)}
              className={INPUT_CLASS}
            >
              {input.entityTypeId === '' && (
                <option value="" disabled>
                  Select a type
                </option>
              )}
              {entityTypes.map((type) => (
                <option key={type.id} value={type.id}>
                  {type.name}
                </option>
              ))}
            </select>
          )}
        </FormField>
        <LocationPicker
          value={input.parentId}
          onChange={(parentId) => set('parentId', parentId)}
          excludeSubtreeOf={initial?.id}
          allowNone
        />
        <FormField label="Quantity" error={quantityError}>
          {(control) => (
            <input
              {...control}
              type="number"
              min={0}
              step="any"
              inputMode="decimal"
              value={quantityText}
              onChange={(event) => setQuantityText(event.target.value)}
              className={INPUT_CLASS}
            />
          )}
        </FormField>
        {longText('Description', 'description')}
      </Fieldset>

      <div className="rounded-lg border border-border bg-surface p-4">
        <TagPicker value={input.tagIds} onChange={(tagIds) => set('tagIds', tagIds)} />
      </div>

      <CollapsibleFieldset
        legend="Purchase"
        open={isOpen('purchase')}
        onToggle={() => toggle('purchase')}
      >
        {text('Purchase date', 'purchaseDate', 'date')}
        {text('Purchased from', 'purchaseFrom')}
        {moneyField('Purchase price', 'purchasePriceCents')}
      </CollapsibleFieldset>

      <CollapsibleFieldset
        legend="Warranty"
        open={isOpen('warranty')}
        onToggle={() => toggle('warranty')}
      >
        <div className="flex items-end sm:col-span-2">
          {checkbox('Lifetime warranty', 'lifetimeWarranty')}
        </div>
        {text('Warranty expires', 'warrantyExpires', 'date')}
        {text('Warranty details', 'warrantyDetails')}
      </CollapsibleFieldset>

      <CollapsibleFieldset legend="Sold" open={isOpen('sold')} onToggle={() => toggle('sold')}>
        {text('Sold date', 'soldDate', 'date')}
        {text('Sold to', 'soldTo')}
        {moneyField('Sold price', 'soldPriceCents')}
        {longText('Sold notes', 'soldNotes')}
      </CollapsibleFieldset>

      <Fieldset legend="Details">
        {text('Manufacturer', 'manufacturer')}
        {text('Model number', 'modelNumber')}
        {text('Serial number', 'serialNumber')}
        <div className="flex flex-col justify-end gap-2">
          {checkbox('Insured', 'insured')}
          {checkbox('Archived', 'archived')}
        </div>
        {longText('Notes', 'notes')}
      </Fieldset>

      <div className="flex items-center justify-end gap-3">
        {props.secondaryAction ?? (
          <Link
            to={props.cancelTo ?? defaultCancelTo(props)}
            className="rounded-md px-3 py-1.5 text-sm text-muted hover:text-text"
          >
            Cancel
          </Link>
        )}
        <button
          type="submit"
          disabled={invalid || submitting}
          className="rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-accent-text disabled:opacity-50"
        >
          {props.submitLabel ?? 'Save'}
        </button>
      </div>
    </form>
  );
};
