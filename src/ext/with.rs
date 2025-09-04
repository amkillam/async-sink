use core::fmt;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use tokio_stream::Stream;

use super::Sink;

/// Sink for the [`with`](super::SinkExt::with) method.
#[must_use = "sinks do nothing unless polled"]
pub struct With<Si, Item, U, Fut, F, E> {
    sink: Si,
    f: F,
    state: Option<Fut>,
    _phantom_e: PhantomData<E>,
    _phantom_item: PhantomData<fn(U) -> Item>,
}

impl<Si: Unpin, Item, U, Fut: Unpin, F, E> Unpin for With<Si, Item, U, Fut, F, E> {}

impl<Si, Item, U, Fut, F, E> fmt::Debug for With<Si, Item, U, Fut, F, E>
where
    Si: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("With")
            .field("sink", &self.sink)
            .field("state", &self.state)
            .finish()
    }
}

impl<Si, Item, U, Fut, F, E> With<Si, Item, U, Fut, F, E>
where
    Si: Sink<Item>,
    F: FnMut(U) -> Fut,
    E: From<Si::Error>,
    Fut: Future<Output = Result<Item, E>>,
{
    pub(super) fn new(sink: Si, f: F) -> Self {
        Self {
            state: None,
            sink,
            f,
            _phantom_item: PhantomData,
            _phantom_e: PhantomData,
        }
    }
}

impl<Si, Item, U, Fut, F, E> Clone for With<Si, Item, U, Fut, F, E>
where
    Si: Clone,
    F: Clone,
    Fut: Clone,
{
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            sink: self.sink.clone(),
            f: self.f.clone(),
            _phantom_item: PhantomData,
            _phantom_e: PhantomData,
        }
    }
}

// Forwarding impl of Stream from the underlying sink
impl<S, Item, U, Fut, F, E> Stream for With<S, Item, U, Fut, F, E>
where
    S: Stream + Sink<Item>,
    F: FnMut(U) -> Fut,
    Fut: Future<Output = Result<Item, E>>,
    E: From<S::Error>,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        let sink = unsafe { Pin::new_unchecked(&mut this.sink) };
        sink.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sink.size_hint()
    }
}

impl<Si, Item, U, Fut, F, E> With<Si, Item, U, Fut, F, E>
where
    Si: Sink<Item>,
    F: FnMut(U) -> Fut,
    Fut: Future<Output = Result<Item, E>>,
    E: From<Si::Error>,
{
    /// Acquires a reference to the underlying sink.
    pub fn get_ref(&self) -> &Si {
        &self.sink
    }

    /// Acquires a mutable reference to the underlying sink.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// sink which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut Si {
        &mut self.sink
    }

    /// Acquires a pinned mutable reference to the underlying sink.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// sink which may otherwise confuse this combinator.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut Si> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) }
    }

    /// Consumes this combinator, returning the underlying sink.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> Si {
        self.sink
    }

    /// Completes the processing of previous item if any.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        let this = unsafe { self.get_unchecked_mut() };

        if let Some(fut) = this.state.as_mut() {
            let item_res = match unsafe { Pin::new_unchecked(fut) }.poll(cx) {
                Poll::Ready(res) => res,
                Poll::Pending => return Poll::Pending,
            };
            this.state = None;
            let item = match item_res {
                Ok(item) => item,
                Err(e) => return Poll::Ready(Err(e)),
            };
            let sink = unsafe { Pin::new_unchecked(&mut this.sink) };
            if let Err(e) = sink.start_send(item) {
                return Poll::Ready(Err(e.into()));
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl<Si, Item, U, Fut, F, E> Sink<U> for With<Si, Item, U, Fut, F, E>
where
    Si: Sink<Item>,
    F: FnMut(U) -> Fut,
    Fut: Future<Output = Result<Item, E>>,
    E: From<Si::Error>,
{
    type Error = E;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }
        let sink = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) };
        match sink.poll_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn start_send(self: Pin<&mut Self>, item: U) -> Result<(), Self::Error> {
        let this = unsafe { self.get_unchecked_mut() };

        assert!(this.state.is_none());
        this.state = Some((this.f)(item));
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        };
        let sink = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) };
        match sink.poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        };
        let sink = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) };
        match sink.poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }
}
