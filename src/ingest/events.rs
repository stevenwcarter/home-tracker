//! The per-batch change signal behind the SSE progress stream.
//!
//! An event only says *what* changed (a photo, an item or the batch, by id);
//! the client refetches the batch over GraphQL for the new state, so a
//! dropped or lagged event costs nothing but a refetch.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::broadcast;

/// How many events a slow subscriber may fall behind before it lags.
const CAPACITY: usize = 64;

/// Which kind of ingest row an [`IngestEvent`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Photo,
    Item,
    Batch,
}

impl EventKind {
    /// The SSE event name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Item => "item",
            Self::Batch => "batch",
        }
    }
}

/// One status change: row `id` of `kind` changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestEvent {
    pub kind: EventKind,
    pub id: String,
}

impl IngestEvent {
    pub fn new(kind: EventKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}

/// One broadcast channel per batch that someone is watching.
#[derive(Default)]
pub struct IngestEvents {
    channels: Mutex<HashMap<String, broadcast::Sender<IngestEvent>>>,
}

impl IngestEvents {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// A receiver of batch `batch_id`'s future events, opening its channel
    /// if nobody watches it yet.
    pub fn subscribe(&self, batch_id: &str) -> broadcast::Receiver<IngestEvent> {
        self.channels()
            .entry(batch_id.to_owned())
            .or_insert_with(|| broadcast::channel(CAPACITY).0)
            .subscribe()
    }

    /// Sends `event` to batch `batch_id`'s subscribers. With none left the
    /// channel is dropped, so batches nobody watches hold no memory; a later
    /// subscriber opens a fresh one (and loads the current state anyway).
    pub fn publish(&self, batch_id: &str, event: IngestEvent) {
        let mut channels = self.channels();
        let unwatched = channels
            .get(batch_id)
            .is_some_and(|sender| sender.send(event).is_err());
        if unwatched {
            channels.remove(batch_id);
        }
    }

    /// Ends batch `batch_id`'s streams: dropping the sender makes every
    /// receiver see the channel closed once it has read what was sent.
    pub fn close(&self, batch_id: &str) {
        self.channels().remove(batch_id);
    }

    /// Tells batch `batch_id`'s streams it was deleted: a batch event, so a
    /// viewer refetches and finds it gone, then the end.
    pub fn removed(&self, batch_id: &str) {
        self.publish(batch_id, IngestEvent::new(EventKind::Batch, batch_id));
        self.close(batch_id);
    }

    /// A poisoned lock only means a publisher panicked mid-insert; the map
    /// is still usable.
    fn channels(&self) -> MutexGuard<'_, HashMap<String, broadcast::Sender<IngestEvent>>> {
        self.channels.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast::error::TryRecvError;

    use super::*;

    #[test]
    fn subscribers_get_their_batchs_events_only() {
        let events = IngestEvents::new();
        let mut first = events.subscribe("b1");
        let mut again = events.subscribe("b1");
        let mut other = events.subscribe("b2");
        events.publish("b1", IngestEvent::new(EventKind::Photo, "p1"));
        let expected = IngestEvent::new(EventKind::Photo, "p1");
        assert_eq!(first.try_recv(), Ok(expected.clone()));
        assert_eq!(again.try_recv(), Ok(expected));
        assert_eq!(other.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn close_ends_the_stream_after_what_was_sent() {
        let events = IngestEvents::new();
        let mut receiver = events.subscribe("b1");
        events.publish("b1", IngestEvent::new(EventKind::Batch, "b1"));
        events.close("b1");
        assert_eq!(
            receiver.try_recv(),
            Ok(IngestEvent::new(EventKind::Batch, "b1"))
        );
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Closed));
    }

    #[test]
    fn removed_sends_a_batch_event_then_ends_the_stream() {
        let events = IngestEvents::new();
        let mut receiver = events.subscribe("b1");
        events.removed("b1");
        assert_eq!(
            receiver.try_recv(),
            Ok(IngestEvent::new(EventKind::Batch, "b1"))
        );
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Closed));
    }

    #[test]
    fn a_channel_nobody_watches_is_dropped() {
        let events = IngestEvents::new();
        // Publishing to a batch nobody ever watched opens nothing.
        events.publish("b1", IngestEvent::new(EventKind::Item, "i1"));
        assert!(events.channels().is_empty());
        drop(events.subscribe("b1"));
        events.publish("b1", IngestEvent::new(EventKind::Item, "i1"));
        assert!(events.channels().is_empty());
        assert_eq!(EventKind::Item.as_str(), "item");
    }
}
