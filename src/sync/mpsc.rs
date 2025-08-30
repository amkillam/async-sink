//! [`Sink`] implementations for [`tokio`](https://docs.rs/tokio)'s MPSC channels
//!
//! [`Sink`]: trait@crate::Sink

use crate::Sink;
use alloc::boxed::Box;
use core::fmt;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, UnboundedReceiverStream};

pub use tokio::sync::mpsc::error;
pub use tokio::sync::mpsc::error::*;

/// A thin wrapper around [`tokio::sync::mpsc::Sender`] that implements [`Sync`].
///
/// [`tokio::sync::mpsc::Sender`]: struct@tokio::sync::mpsc::Sender
/// [`Sink`]: trait@crate::Sink
pub struct SenderSink<T> {
    pub(crate) inner: mpsc::Sender<T>,
    // Future created by `reserve()` to register wakers for readiness.
    // Stored across polls to maintain proper waker registration.
    #[allow(clippy::type_complexity)]
    reserve_fut: Option<
        Pin<
            Box<
                dyn core::future::Future<Output = Result<(), mpsc::error::TrySendError<()>>>
                    + 'static,
            >,
        >,
    >,
}

impl<T> SenderSink<T> {
    /// Create a new `SenderSink` wrapping the provided `Sender`.
    #[inline(always)]
    pub fn new(sender: mpsc::Sender<T>) -> Self {
        Self {
            inner: sender,
            reserve_fut: None,
        }
    }

    /// Get back the inner `Sender`.
    #[inline(always)]
    pub fn into_inner(self) -> mpsc::Sender<T> {
        self.inner
    }
}

impl<T> AsRef<mpsc::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn as_ref(&self) -> &mpsc::Sender<T> {
        &self.inner
    }
}

impl<T> AsMut<mpsc::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut mpsc::Sender<T> {
        &mut self.inner
    }
}

impl<T> Clone for SenderSink<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            reserve_fut: None,
        }
    }
}

impl<T> fmt::Debug for SenderSink<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderSink")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T> From<mpsc::Sender<T>> for SenderSink<T> {
    #[inline(always)]
    fn from(sender: mpsc::Sender<T>) -> Self {
        Self::new(sender)
    }
}

impl<T: 'static> Sink<T> for SenderSink<T> {
    type Error = mpsc::error::TrySendError<()>;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.as_mut().get_mut();

        if this.inner.is_closed() {
            return Poll::Ready(Err(mpsc::error::TrySendError::Closed(())));
        }
        if this.inner.capacity() > 0 {
            // If we previously spawned a reserve future, drop it now.
            this.reserve_fut = None;
            return Poll::Ready(Ok(()));
        }

        if this.reserve_fut.is_none() {
            let sender = this.inner.clone();
            this.reserve_fut = Some(Box::pin(async move {
                match sender.reserve().await {
                    Ok(permit) => {
                        drop(permit);
                        Ok(())
                    }
                    Err(e) => Err(mpsc::error::TrySendError::Full(())),
                }
            }));
        }

        match this
            .reserve_fut
            .as_mut()
            .expect("reserve_fut must be set")
            .as_mut()
            .poll(cx)
        {
            Poll::Ready(Ok(())) => {
                this.reserve_fut = None;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => {
                this.reserve_fut = None;
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.get_mut().inner.try_send(item).map_err(|e| match e {
            mpsc::error::TrySendError::Closed(_) => mpsc::error::TrySendError::Closed(()),
            mpsc::error::TrySendError::Full(_) => mpsc::error::TrySendError::Full(()),
        })
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
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
    type Error = mpsc::error::SendError<T>;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // The unbounded sender is always ready to send
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.get_mut().0.send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
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
    (
        UnboundedSenderSink::new(tx),
        UnboundedReceiverStream::new(rx),
    )
}
