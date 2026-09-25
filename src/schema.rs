// @generated automatically by Diesel CLI.

diesel::table! {
    attachments (id) {
        id -> Text,
        entity_id -> Text,
        kind -> Text,
        is_primary -> Bool,
        title -> Text,
        mime_type -> Text,
        sha256 -> Text,
        size_bytes -> BigInt,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    entities (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        entity_type_id -> Text,
        parent_id -> Nullable<Text>,
        archived -> Bool,
        asset_id -> BigInt,
        import_ref -> Nullable<Text>,
        notes -> Nullable<Text>,
        quantity -> Double,
        insured -> Bool,
        serial_number -> Nullable<Text>,
        model_number -> Nullable<Text>,
        manufacturer -> Nullable<Text>,
        lifetime_warranty -> Bool,
        warranty_expires -> Nullable<Date>,
        warranty_details -> Nullable<Text>,
        purchase_date -> Nullable<Date>,
        purchase_from -> Nullable<Text>,
        purchase_price_cents -> BigInt,
        sold_date -> Nullable<Date>,
        sold_to -> Nullable<Text>,
        sold_price_cents -> BigInt,
        sold_notes -> Nullable<Text>,
        sync_child_entity_locations -> Bool,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    entity_fields (id) {
        id -> Text,
        entity_id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        kind -> Text,
        text_value -> Nullable<Text>,
        number_value -> Nullable<BigInt>,
        boolean_value -> Bool,
        time_value -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    entity_templates (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        notes -> Nullable<Text>,
        default_quantity -> Double,
        default_insured -> Bool,
        default_name -> Nullable<Text>,
        default_description -> Nullable<Text>,
        default_manufacturer -> Nullable<Text>,
        default_model_number -> Nullable<Text>,
        default_lifetime_warranty -> Bool,
        default_warranty_details -> Nullable<Text>,
        include_warranty_fields -> Bool,
        include_purchase_fields -> Bool,
        include_sold_fields -> Bool,
        default_tag_ids -> Nullable<Text>,
        location_id -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    entity_types (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        icon -> Nullable<Text>,
        is_location -> Bool,
        default_template_id -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    settings (key) {
        key -> Text,
        value -> Text,
    }
}

diesel::table! {
    tag_entities (tag_id, entity_id) {
        tag_id -> Text,
        entity_id -> Text,
    }
}

diesel::table! {
    tags (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        color -> Nullable<Text>,
        icon -> Nullable<Text>,
        parent_id -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    template_fields (id) {
        id -> Text,
        template_id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        kind -> Text,
        text_value -> Nullable<Text>,
        number_value -> Nullable<BigInt>,
        boolean_value -> Bool,
        time_value -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    thumbnails (attachment_id, size) {
        attachment_id -> Text,
        size -> Integer,
        mime_type -> Text,
        width -> Integer,
        height -> Integer,
        data -> Binary,
        created_at -> Timestamp,
    }
}

diesel::joinable!(attachments -> entities (entity_id));
diesel::joinable!(entities -> entity_types (entity_type_id));
diesel::joinable!(entity_fields -> entities (entity_id));
diesel::joinable!(entity_templates -> entities (location_id));
diesel::joinable!(entity_types -> entity_templates (default_template_id));
diesel::joinable!(tag_entities -> entities (entity_id));
diesel::joinable!(tag_entities -> tags (tag_id));
diesel::joinable!(template_fields -> entity_templates (template_id));
diesel::joinable!(thumbnails -> attachments (attachment_id));

diesel::allow_tables_to_appear_in_same_query!(
    attachments,
    entities,
    entity_fields,
    entity_templates,
    entity_types,
    settings,
    tag_entities,
    tags,
    template_fields,
    thumbnails,
);
