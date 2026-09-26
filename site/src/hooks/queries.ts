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

export const GET_ENTITY = gql`
  query GetEntity($id: ID!) {
    entity(id: $id) {
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
  }
  ${ENTITY_LIST_ITEM_FIELDS}
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
