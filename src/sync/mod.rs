//! [`Sink`] implementations for various [`tokio`](https://docs.rs/tokio) synchronization primitives.
//!
//! [`Sink`]: trait@crate::Sink

pub mod mpsc;
pub mod oneshot;
