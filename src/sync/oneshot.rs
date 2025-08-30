use tokio::sync::oneshot;
use core::{pin::Pin, task::{Context, Poll}};
use crate::Sink;

/// A thin wrapper around [`tokio::sync::oneshot::Sender`] that implements [`Sync`].
///
/// [`tokio::sync::oneshot::Sender`]: struct@tokio::sync::oneshot::Sender
/// [`Sink`]: trait@crate::Sink
#[derive(Debug)]
#[repr(transparent)]
pub struct SenderSink<T>(pub Option<oneshot::Sender<T>>);

impl<T> SenderSink<T> {
    /// Create a new `SenderSink` wrapping the provided `Sender`.
    #[inline(always)]
    pub fn new(sender: oneshot::Sender<T>) -> Self {
        Self(Some(sender))
    }

    /// Get back the inner `Sender`.
    #[inline(always)]
    pub fn into_inner(self) -> Option<oneshot::Sender<T>> {
        self.0
    }

}


impl<T> From<oneshot::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn from(sender: oneshot::Sender<T>) -> Self {
        Self::new(sender)
    }
}

impl<T> Sink<T> for SenderSink<T> {
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

pub fn oneshot<T>() -> (SenderSink<T>, tokio_stream_util::sync::oneshot::ReceiverStream<T>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (SenderSink::new(tx), tokio_stream_util::sync::oneshot::ReceiverStream::new(rx))
}
