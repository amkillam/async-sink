//! A wrapper [`Sink`] implementation for [`tokio::sync::oneshot`].
//!
//! [`Sink`]: trait@crate::Sink
//! [`tokio::sync::oneshot`]: https://docs.rs/tokio/latest

use crate::Sink;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;

/// A thin wrapper around [`tokio::sync::oneshot::Sender`] that implements [`Sync`].
///
/// [`tokio::sync::oneshot::Sender`]: struct@tokio::sync::oneshot::Sender
/// [`Sink`]: trait@crate::Sink
#[derive(Debug)]
#[repr(transparent)]
pub struct Sender<T>(pub Option<oneshot::Sender<T>>);

impl<T> Sender<T> {
    /// Create a new `Sender` wrapping the provided `Sender`.
    #[inline(always)]
    pub fn new(sender: oneshot::Sender<T>) -> Self {
        Self(Some(sender))
    }

    /// Get back the inner `Sender`.
    #[inline(always)]
    pub fn into_inner(self) -> Option<oneshot::Sender<T>> {
        self.0
    }

    #[inline(always)]
    pub fn send(self, t: T) -> Result<(), T> {
        match self.0 {
            Some(sender) => sender.send(t),
            None => Err(t),
        }
    }

    #[inline(always)]
    pub async fn closed(&mut self) {
        if let Some(sender) = self.0.as_mut() {
            sender.closed().await;
        }
    }

    #[inline(always)]
    pub fn is_closed(&self) -> bool {
        match self.0.as_ref() {
            None => true,
            Some(inner) => inner.is_closed(),
        }
    }

    #[inline(always)]
    pub fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self.0.as_mut() {
            Some(sender) => sender.poll_closed(cx),
            None => Poll::Ready(()),
        }
    }
}

impl<T> From<oneshot::Sender<T>> for Sender<T> {
    #[inline(always)]
    fn from(sender: oneshot::Sender<T>) -> Self {
        Self::new(sender)
    }
}

impl<T> Sink<T> for Sender<T> {
    type Error = T;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = cx;
        // oneshot::Sender can accept exactly one message; we allow attempting to send
        // and let start_send error if it's already been used.
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        match self.get_mut().0.take() {
            Some(sender) => sender.send(item),
            None => Err(item),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = cx;
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = cx;
        let _ = self.get_mut().0.take();
        Poll::Ready(Ok(()))
    }
}

/// Create a new oneshot channel, returning a [`Sender`] and a [`ReceiverStream`].
///
/// [`Sender`]: struct@Sender
/// [`ReceiverStream`]: struct@tokio_stream_util::sync::oneshot::Receiver
pub fn channel<T>() -> (
    Sender<T>,
    tokio_stream_util::sync::oneshot::ReceiverStream<T>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (
        Sender::new(tx),
        tokio_stream_util::sync::oneshot::ReceiverStream::new(rx),
    )
}
