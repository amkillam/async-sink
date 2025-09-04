use core::pin::Pin;
use core::task::{Context, Poll};
use tokio_stream::Stream;

use super::Sink;

/// Sink for the [`sink_map_err`](super::SinkExt::sink_map_err) method.
#[derive(Debug, Clone)]
#[must_use = "sinks do nothing unless polled"]
pub struct SinkMapErr<Si, F> {
    sink: Si,
    f: Option<F>,
}

impl<Si, F> SinkMapErr<Si, F> {
    pub(super) fn new(sink: Si, f: F) -> Self {
        Self { sink, f: Some(f) }
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
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) }
    }

    /// Consumes this combinator, returning the underlying sink.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> Si {
        self.sink
    }

    fn take_f(self: Pin<&mut Self>) -> F {
        unsafe { self.get_unchecked_mut() }
            .f
            .take()
            .expect("polled MapErr after completion")
    }
}

impl<Si, F, E, Item> Sink<Item> for SinkMapErr<Si, F>
where
    E: core::error::Error,
    Si: Sink<Item>,
    F: FnOnce(Si::Error) -> E,
{
    type Error = E;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().get_pin_mut().poll_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(self.take_f()(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        match self.as_mut().get_pin_mut().start_send(item) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.take_f()(e)),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().get_pin_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(self.take_f()(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().get_pin_mut().poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(self.take_f()(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

// Forwarding impl of Stream from the underlying sink
impl<S: Stream, F> Stream for SinkMapErr<S, F> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.as_mut().get_pin_mut().poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sink.size_hint()
    }
}
