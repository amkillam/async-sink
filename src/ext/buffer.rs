use super::Sink;
use alloc::collections::VecDeque;
use core::pin::Pin;
use core::task::{Context, Poll};
use tokio_stream::Stream;

/// Sink for the [`buffer`](super::SinkExt::buffer) method.
#[derive(Debug)]
#[must_use = "sinks do nothing unless polled"]
pub struct Buffer<Si, Item> {
    sink: Si,
    buf: VecDeque<Item>,

    // Track capacity separately from the `VecDeque`, which may be rounded up
    capacity: usize,
}

impl<Si: Unpin, Item> Unpin for Buffer<Si, Item> {}

impl<Si: Sink<Item>, Item> Buffer<Si, Item> {
    pub(super) fn new(sink: Si, capacity: usize) -> Self {
        Self {
            sink,
            buf: VecDeque::with_capacity(capacity),
            capacity,
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
        unsafe { self.map_unchecked_mut(|this| &mut this.sink) }
    }

    /// Consumes this combinator, returning the underlying sink.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> Si {
        self.sink
    }

    fn try_empty_buffer(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Si::Error>> {
        let this = unsafe { self.as_mut().get_unchecked_mut() };
        let mut sink = unsafe { Pin::new_unchecked(&mut this.sink) };

        match sink.as_mut().poll_ready(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }

        while let Some(item) = this.buf.pop_front() {
            if let Err(e) = sink.as_mut().start_send(item) {
                return Poll::Ready(Err(e));
            }

            if !this.buf.is_empty() {
                match sink.as_mut().poll_ready(cx) {
                    Poll::Ready(Ok(())) => {}
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
        Poll::Ready(Ok(()))
    }
}

// Forwarding impl of Stream from the underlying sink
impl<S, Item> Stream for Buffer<S, Item>
where
    S: Sink<Item> + Stream,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<S::Item>> {
        unsafe { self.map_unchecked_mut(|this| &mut this.sink) }.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sink.size_hint()
    }
}

impl<Si: Sink<Item>, Item> Sink<Item> for Buffer<Si, Item> {
    type Error = Si::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = unsafe { self.as_mut().get_unchecked_mut() };
        if this.capacity == 0 {
            return unsafe { Pin::new_unchecked(&mut this.sink) }.poll_ready(cx);
        }

        if this.buf.len() >= this.capacity {
            match self.try_empty_buffer(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let this = unsafe { self.get_unchecked_mut() };
        if this.capacity == 0 {
            unsafe { Pin::new_unchecked(&mut this.sink) }.start_send(item)
        } else {
            this.buf.push_back(item);
            Ok(())
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().try_empty_buffer(cx) {
            Poll::Ready(Ok(())) => (),
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }
        let this = unsafe { self.get_unchecked_mut() };
        debug_assert!(this.buf.is_empty());
        unsafe { Pin::new_unchecked(&mut this.sink) }.poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().try_empty_buffer(cx) {
            Poll::Ready(Ok(())) => (),
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }
        let this = unsafe { self.get_unchecked_mut() };
        debug_assert!(this.buf.is_empty());
        unsafe { Pin::new_unchecked(&mut this.sink) }.poll_close(cx)
    }
}
