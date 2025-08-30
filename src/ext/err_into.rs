use super::Sink;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use tokio_stream::Stream;
use tokio_stream_util::FusedStream;

/// Sink for the [`sink_err_into`](super::SinkExt::sink_err_into) method.
#[derive(Debug)]
#[must_use = "sinks do nothing unless polled"]
pub struct SinkErrInto<Si, Item, E> {
    sink: Si,
    _phantom: PhantomData<fn(Item) -> E>,
}

impl<Si, Item, E> SinkErrInto<Si, Item, E> {
    pub(super) fn new(sink: Si) -> Self {
        Self {
            sink,
            _phantom: PhantomData,
        }
    }

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
        unsafe { self.map_unchecked_mut(|s| &mut s.sink) }
    }

    /// Consumes this combinator, returning the underlying sink.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> Si {
        self.sink
    }
}

impl<Si, Item, E> Sink<Item> for SinkErrInto<Si, Item, E>
where
    Si: Sink<Item>,
    Si::Error: Into<E>,
{
    type Error = E;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let sink = unsafe { self.map_unchecked_mut(|s| &mut s.sink) };
        match sink.poll_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let sink = unsafe { self.map_unchecked_mut(|s| &mut s.sink) };
        match sink.start_send(item) {
            Ok(()) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let sink = unsafe { self.map_unchecked_mut(|s| &mut s.sink) };
        match sink.poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let sink = unsafe { self.map_unchecked_mut(|s| &mut s.sink) };
        match sink.poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }
}

// Forwarding impl of Stream from the underlying sink
impl<Si, Item, E> Stream for SinkErrInto<Si, Item, E>
where
    Si: Sink<Item> + Stream,
{
    type Item = Si::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.sink) }.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sink.size_hint()
    }
}

impl<Si, Item, E> FusedStream for SinkErrInto<Si, Item, E>
where
    Si: Sink<Item> + FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.sink.is_terminated()
    }
}
