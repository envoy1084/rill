//! Stream type returned by watch RPCs.

use std::pin::Pin;

use futures::Stream;

use crate::error::Result;

/// A boxed, sendable stream of SDK values; each item is a converted
/// server message or the error that ended the stream.
pub type EventStream<T> = Pin<Box<dyn Stream<Item = Result<T>> + Send + 'static>>;
