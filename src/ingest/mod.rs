//! The AI ingest pipeline: the runner that describes each staged photo and
//! synthesises each item's suggestion, the lenient parsing of the model's
//! answers, and the per-batch change events the progress stream relays.
//! Staging and review storage live in [`crate::svc::ingest`].

pub mod events;
pub mod parse;
pub mod runner;
