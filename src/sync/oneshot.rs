use tokio::sync::oneshot;
use core::{pin::Pin, task::{Context, Poll}};
use crate::Sink;

/// A thin wrapper around [`tokio::sync::oneshot::Sender`] that implements [`Sync`].
///
/// [`tokio::sync::oneshot::Sender`]: struct@tokio::sync::oneshot::Sender
/// [`Sink`]: trait@crate::Sink
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct SenderSink<T>(pub oneshot::Sender<T>);

impl<T> SenderSink<T> {
    /// Create a new `SenderSink` wrapping the provided `Sender`.
    #[inline(always)]
    pub fn new(sender: oneshot::Sender<T>) -> Self {
        Self(sender)
    }

    /// Get back the inner `Sender`.
    #[inline(always)]
    pub fn into_inner(self) -> oneshot::Sender<T> {
        self.0
    }

    /// Closes the sending half of a channel without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered. Any
    /// outstanding [`Permit`] values will still be able to send messages.
    ///
    /// [`Permit`]: struct@tokio::sync::mpsc::Permit
    #[inline(always)]
    pub fn close(&self) {
        self.0.close_channel();
    }
}

impl<T> AsRef<oneshot::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn as_ref(&self) -> &oneshot::Sender<T> {
        &self.0
    }
}

impl<T> AsMut<oneshot::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut oneshot::Sender<T> {
        &mut self.0
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
        if self.0.is_closed() {
            Poll::Ready(Err(self.0.into_inner().unwrap_err()))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
       self.0.send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.get_mut().0).poll_close(cx)
    }
}

pub fn oneshot<T>() -> (SenderSink<T>, tokio_stream_util::sync::oneshot::ReceiverStream<T>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (SenderSink::new(tx), tokio_stream_util::sync::oneshot::ReceiverStream::new(rx))
}
