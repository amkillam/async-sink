use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, UnboundedReceiverStream};
use core::{pin::Pin, task::{Context, Poll}};
use crate::Sink;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendError;

/// A thin wrapper around [`tokio::sync::mpsc::Sender`] that implements [`Sync`].
///
/// [`tokio::sync::mpsc::Sender`]: struct@tokio::sync::mpsc::Sender
/// [`Sink`]: trait@crate::Sink
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct SenderSink<T>(pub mpsc::Sender<T>);

impl<T> SenderSink<T> {
    /// Create a new `SenderSink` wrapping the provided `Sender`.
    #[inline(always)]
    pub fn new(sender: mpsc::Sender<T>) -> Self {
        Self(sender)
    }

    /// Get back the inner `Sender`.
    #[inline(always)]
    pub fn into_inner(self) -> mpsc::Sender<T> {
        self.0
    }
}

impl<T> AsRef<mpsc::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn as_ref(&self) -> &mpsc::Sender<T> {
        &self.0
    }
}

impl<T> AsMut<mpsc::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut mpsc::Sender<T> {
        &mut self.0
    }
}

impl<T> From<mpsc::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn from(sender: mpsc::Sender<T>) -> Self {
        Self::new(sender)
    }
}

impl<T> Sink<T> for SenderSink<T> {
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.0.is_closed() {
            return Poll::Ready(Err(SendError));
        }
        if self.0.capacity() > 0 {
            return Poll::Ready(Ok(()));
        }
        match self.get_mut().0.poll_reserve(cx) {
            Poll::Ready(Ok(permit)) => {
                drop(permit);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(_)) => Poll::Ready(Err(SendError)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.get_mut().0.try_send(item).map_err(|_| SendError)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = cx;
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().0.close_channel();
        let _ = cx;
        Poll::Ready(Ok(()))
    }
}

/// A thin wrapper around [`tokio::sync::mpsc::UnboundedSender`] that implements [`Sync`].
///
/// [`tokio::sync::mpsc::UnboundedSender`]: struct@tokio::sync::mpsc::UnboundedSender
/// [`Sink`]: trait@crate::Sink
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UnboundedSenderSink<T>(pub mpsc::UnboundedSender<T>);

impl<T> UnboundedSenderSink<T> {
    /// Create a new `UnboundedSenderSink` wrapping the provided `UnboundedSender`.
    #[inline(always)]
    pub fn new(sender: mpsc::UnboundedSender<T>) -> Self {
        Self(sender)
    }

    /// Get back the inner `UnboundedSender`.
    #[inline(always)]
    pub fn into_inner(self) -> mpsc::UnboundedSender<T> {
        self.0
    }

  
}

impl<T> AsRef<mpsc::UnboundedSender<T>> for UnboundedSenderSink<T> {
    #[inline(always)]
    fn as_ref(&self) -> &mpsc::UnboundedSender<T> {
        &self.0
    }
}

impl<T> AsMut<mpsc::UnboundedSender<T>> for UnboundedSenderSink<T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut mpsc::UnboundedSender<T> {
        &mut self.0
    }
}

impl<T> From<mpsc::UnboundedSender<T>> for UnboundedSenderSink<T> {
    #[inline(always)]
    fn from(sender: mpsc::UnboundedSender<T>) -> Self {
        Self::new(sender)
    }
}

impl<T> Sink<T> for UnboundedSenderSink<T> {
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = cx;
        // The unbounded sender is always ready to send
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.get_mut().0.send(item).map_err(|_| SendError)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = cx;
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().0.close_channel();
        let _ = cx;
        Poll::Ready(Ok(()))
    }
}

/// Creates a bounded MPSC channel, returning the sender wrapped in a [`SenderSink`] and the receiver wrapped in a [`ReceiverStream`].
///
/// [`SenderSink`]: struct@SenderSink
/// [`ReceiverStream`]: struct@tokio_stream::wrappers::ReceiverStream
#[inline(always)]
pub fn channel<T>(buffer: usize) -> (SenderSink<T>, ReceiverStream<T>) {
    let (tx, rx) = mpsc::channel(buffer);
    (SenderSink::new(tx), ReceiverStream::new(rx))
}

/// Creates an unbounded MPSC channel, returning the sender wrapped in a [`UnboundedSenderSink`] and the receiver wrapped in a [`UnboundedReceiverStream`].
///
/// [`UnboundedSenderSink`]: struct@UnboundedSenderSink
/// [`UnboundedReceiverStream`]: struct@tokio_stream::wrappers::UnboundedReceiverStream
#[inline(always)]
pub fn unbounded_channel<T>() -> (UnboundedSenderSink<T>, UnboundedReceiverStream<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (UnboundedSenderSink::new(tx), UnboundedReceiverStream::new(rx))
}
