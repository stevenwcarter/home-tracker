import { gql } from '@apollo/client';

export const GET_SUMMARY = gql`
  query GetSummary {
    summary {
      totalValueCents
      currency
      totalItems
      totalLocations
      totalTags
    }
  }
`;

export const GET_LOCATIONS = gql`
  query GetLocations {
    locations {
      id
      name
      parentId
      archived
    }
  }
`;

/** Shared shape for an entity as it appears in a list, not a detail page. */
const ENTITY_LIST_ITEM_FIELDS = gql`
  fragment EntityListItemFields on Entity {
    id
    name
    assetId
    quantity
    purchasePriceCents
    archived
    primaryPhoto {
      thumbnailUrl
    }
    entityType {
      id
      name
      isLocation
    }
  }
`;

/** Every field of an entity's detail page; also what entity mutations return. */
const ENTITY_DETAIL_FIELDS = gql`
  fragment EntityDetailFields on Entity {
    id
    name
    description
    entityType {
      id
      name
      isLocation
    }
    isLocation
    parent {
      id
      name
      parentId
      archived
    }
    parentId
    ancestors {
      id
      name
      parentId
      archived
    }
    childLocations {
      ...EntityListItemFields
    }
    items {
      ...EntityListItemFields
    }
    archived
    assetId
    quantity
    insured
    serialNumber
    modelNumber
    manufacturer
    notes
    lifetimeWarranty
    warrantyExpires
    warrantyDetails
    purchaseDate
    purchaseFrom
    purchasePriceCents
    soldDate
    soldTo
    soldPriceCents
    soldNotes
    tags {
      id
      name
      color
    }
    attachments {
      id
      kind
      primary
      title
      mimeType
      url
      thumbnailUrl
    }
    primaryPhoto {
      id
      kind
      primary
      title
      mimeType
      url
      thumbnailUrl
    }
    fields {
      id
      name
      kind
      textValue
      numberValue
      booleanValue
      timeValue
    }
    createdAt
    updatedAt
  }
  ${ENTITY_LIST_ITEM_FIELDS}
`;

export const GET_ENTITY = gql`
  query GetEntity($id: ID!) {
    entity(id: $id) {
      ...EntityDetailFields
    }
  }
  ${ENTITY_DETAIL_FIELDS}
`;

export const SEARCH = gql`
  query Search($query: String!) {
    search(query: $query, limit: 50) {
      ...EntityListItemFields
    }
  }
  ${ENTITY_LIST_ITEM_FIELDS}
`;

export const GET_ROOT_ITEMS = gql`
  query GetRootItems {
    rootItems {
      ...EntityListItemFields
    }
  }
  ${ENTITY_LIST_ITEM_FIELDS}
`;

const ENTITY_TYPE_FIELDS = gql`
  fragment EntityTypeFields on EntityType {
    id
    name
    description
    icon
    isLocation
    entityCount
  }
`;

// `Tag` exposes its parent as an object, not a `parentId` scalar; `useTags`
// flattens it.
const TAG_FIELDS = gql`
  fragment TagFields on Tag {
    id
    name
    description
    color
    icon
    parent {
      id
    }
    entityCount
  }
`;

export const GET_ENTITY_TYPES = gql`
  query GetEntityTypes {
    entityTypes {
      ...EntityTypeFields
    }
  }
  ${ENTITY_TYPE_FIELDS}
`;

export const GET_TAGS = gql`
  query GetTags {
    tags {
      ...TagFields
    }
  }
  ${TAG_FIELDS}
`;

export const CREATE_ENTITY = gql`
  mutation CreateEntity($input: EntityInput!) {
    createEntity(input: $input) {
      ...EntityDetailFields
    }
  }
  ${ENTITY_DETAIL_FIELDS}
`;

export const UPDATE_ENTITY = gql`
  mutation UpdateEntity($id: ID!, $input: EntityInput!) {
    updateEntity(id: $id, input: $input) {
      ...EntityDetailFields
    }
  }
  ${ENTITY_DETAIL_FIELDS}
`;

export const DELETE_ENTITY = gql`
  mutation DeleteEntity($id: ID!) {
    deleteEntity(id: $id)
  }
`;

export const CREATE_ENTITY_TYPE = gql`
  mutation CreateEntityType($input: EntityTypeInput!) {
    createEntityType(input: $input) {
      ...EntityTypeFields
    }
  }
  ${ENTITY_TYPE_FIELDS}
`;

export const UPDATE_ENTITY_TYPE = gql`
  mutation UpdateEntityType($id: ID!, $input: EntityTypeInput!) {
    updateEntityType(id: $id, input: $input) {
      ...EntityTypeFields
    }
  }
  ${ENTITY_TYPE_FIELDS}
`;

export const DELETE_ENTITY_TYPE = gql`
  mutation DeleteEntityType($id: ID!) {
    deleteEntityType(id: $id)
  }
`;

export const CREATE_TAG = gql`
  mutation CreateTag($input: TagInput!) {
    createTag(input: $input) {
      ...TagFields
    }
  }
  ${TAG_FIELDS}
`;

export const UPDATE_TAG = gql`
  mutation UpdateTag($id: ID!, $input: TagInput!) {
    updateTag(id: $id, input: $input) {
      ...TagFields
    }
  }
  ${TAG_FIELDS}
`;

export const DELETE_TAG = gql`
  mutation DeleteTag($id: ID!) {
    deleteTag(id: $id)
  }
`;

export const DELETE_ATTACHMENT = gql`
  mutation DeleteAttachment($id: ID!) {
    deleteAttachment(id: $id)
  }
`;

export const SET_PRIMARY_PHOTO = gql`
  mutation SetPrimaryPhoto($attachmentId: ID!) {
    setPrimaryPhoto(attachmentId: $attachmentId) {
      ...EntityDetailFields
    }
  }
  ${ENTITY_DETAIL_FIELDS}
`;
