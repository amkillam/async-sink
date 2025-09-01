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
use tokio::sync::mpsc::{
    self, OwnedPermit, Permit, PermitIterator, WeakSender, WeakUnboundedSender,
};
use tokio_stream::wrappers::{ReceiverStream, UnboundedReceiverStream};

pub use tokio::sync::mpsc::error;
pub use tokio::sync::mpsc::error::*;

/// A thin wrapper around [`tokio::sync::mpsc::Sender`] that implements [`Sync`].
///
/// [`tokio::sync::mpsc::Sender`]: struct@tokio::sync::mpsc::Sender
/// [`Sink`]: trait@crate::Sink
pub struct Sender<T> {
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

// SAFETY: reserve_fut is the only non-Send field, and it is pinned and owned by the struct.
unsafe impl<T> Send for Sender<T> where mpsc::Sender<T>: Send {}

impl<T> Sender<T> {
    /// Create a new `Sender` wrapping the provided `Sender`.
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

    #[inline(always)]
    pub async fn closed(&self) {
        self.inner.closed().await
    }

    #[inline(always)]
    pub async fn reserve_many(&self, n: usize) -> Result<PermitIterator<'_, T>, SendError<()>> {
        self.inner.reserve_many(n).await
    }

    #[inline(always)]
    pub async fn reserve_owned(self) -> Result<OwnedPermit<T>, SendError<()>> {
        self.inner.reserve_owned().await
    }

    #[inline(always)]
    pub async fn reserve(&self) -> Result<Permit<'_, T>, SendError<()>> {
        self.inner.reserve().await
    }

    #[inline(always)]
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.send(value).await
    }

    #[cfg(feature = "time")]
    #[inline(always)]
    pub async fn send_timeout(
        &self,
        value: T,
        timeout: core::time::Duration,
    ) -> Result<(), SendTimeoutError<T>> {
        self.inner.send_timeout(value, timeout).await
    }

    #[inline(always)]
    pub fn blocking_send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.blocking_send(value)
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    #[inline(always)]
    pub fn downgrade(&self) -> WeakSender<T> {
        self.inner.downgrade()
    }

    #[inline(always)]
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    #[inline(always)]
    pub fn max_capacity(&self) -> usize {
        self.inner.max_capacity()
    }

    #[inline(always)]
    pub fn same_channel(&self, other: &Self) -> bool {
        self.inner.same_channel(&other.inner)
    }

    #[inline(always)]
    pub fn strong_count(&self) -> usize {
        self.inner.strong_count()
    }

    #[inline(always)]
    pub fn try_reserve_many(&self, n: usize) -> Result<PermitIterator<'_, T>, TrySendError<()>> {
        self.inner.try_reserve_many(n)
    }

    #[inline(always)]
    pub fn try_reserve_owned(self) -> Result<OwnedPermit<T>, TrySendError<Self>> {
        self.inner.try_reserve_owned().map_err(|e| match e {
            mpsc::error::TrySendError::Closed(e) => mpsc::error::TrySendError::Closed(Self {
                inner: e,
                reserve_fut: None,
            }),
            mpsc::error::TrySendError::Full(e) => mpsc::error::TrySendError::Full(Self {
                inner: e,
                reserve_fut: None,
            }),
        })
    }

    #[inline(always)]
    pub fn try_reserve(&self) -> Result<Permit<'_, T>, TrySendError<()>> {
        self.inner.try_reserve()
    }

    #[inline(always)]
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.inner.try_send(message)
    }

    #[inline(always)]
    pub fn weak_count(&self) -> usize {
        self.inner.weak_count()
    }
}

impl<T> AsRef<mpsc::Sender<T>> for Sender<T> {
    #[inline(always)]
    fn as_ref(&self) -> &mpsc::Sender<T> {
        &self.inner
    }
}

impl<T> AsMut<mpsc::Sender<T>> for Sender<T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut mpsc::Sender<T> {
        &mut self.inner
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            reserve_fut: None,
        }
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sender")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T> From<mpsc::Sender<T>> for Sender<T> {
    #[inline(always)]
    fn from(sender: mpsc::Sender<T>) -> Self {
        Self::new(sender)
    }
}

impl<T: 'static> Sink<T> for Sender<T> {
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
                    Err(_e) => Err(mpsc::error::TrySendError::Full(())),
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
pub struct UnboundedSender<T>(pub mpsc::UnboundedSender<T>);

impl<T> UnboundedSender<T> {
    /// Create a new `UnboundedSender` wrapping the provided `UnboundedSender`.
    #[inline(always)]
    pub fn new(sender: mpsc::UnboundedSender<T>) -> Self {
        Self(sender)
    }

    /// Get back the inner `UnboundedSender`.
    #[inline(always)]
    pub fn into_inner(self) -> mpsc::UnboundedSender<T> {
        self.0
    }

    #[inline(always)]
    pub async fn closed(&self) {
        self.0.closed().await
    }

    #[inline(always)]
    pub fn downgrade(&self) -> WeakUnboundedSender<T> {
        self.0.downgrade()
    }

    #[inline(always)]
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    #[inline(always)]
    pub fn same_channel(&self, other: &Self) -> bool {
        self.0.same_channel(&other.0)
    }

    #[inline(always)]
    pub fn send(&self, message: T) -> Result<(), SendError<T>> {
        self.0.send(message)
    }

    #[inline(always)]
    pub fn strong_count(&self) -> usize {
        self.0.strong_count()
    }

    #[inline(always)]
    pub fn weak_count(&self) -> usize {
        self.0.weak_count()
    }
}

impl<T> AsRef<mpsc::UnboundedSender<T>> for UnboundedSender<T> {
    #[inline(always)]
    fn as_ref(&self) -> &mpsc::UnboundedSender<T> {
        &self.0
    }
}

impl<T> AsMut<mpsc::UnboundedSender<T>> for UnboundedSender<T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut mpsc::UnboundedSender<T> {
        &mut self.0
    }
}

impl<T> From<mpsc::UnboundedSender<T>> for UnboundedSender<T> {
    #[inline(always)]
    fn from(sender: mpsc::UnboundedSender<T>) -> Self {
        Self::new(sender)
    }
}

impl<T> Sink<T> for UnboundedSender<T> {
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

/// Creates a bounded MPSC channel, returning the sender wrapped in a [`Sender`] and the receiver wrapped in a [`ReceiverStream`].
///
/// [`Sender`]: struct@Sender
/// [`ReceiverStream`]: struct@tokio_stream::wrappers::ReceiverStream
#[inline(always)]
pub fn channel<T>(buffer: usize) -> (Sender<T>, ReceiverStream<T>) {
    let (tx, rx) = mpsc::channel(buffer);
    (Sender::new(tx), ReceiverStream::new(rx))
}

/// Creates an unbounded MPSC channel, returning the sender wrapped in a [`UnboundedSender`] and the receiver wrapped in a [`UnboundedReceiverStream`].
///
/// [`UnboundedSender`]: struct@UnboundedSender
/// [`UnboundedReceiverStream`]: struct@tokio_stream::wrappers::UnboundedReceiverStream
#[inline(always)]
pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiverStream<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (UnboundedSender::new(tx), UnboundedReceiverStream::new(rx))
}
