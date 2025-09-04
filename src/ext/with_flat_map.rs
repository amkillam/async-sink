use core::fmt;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use tokio_stream::Stream;

use super::Sink;

/// Sink for the [`with_flat_map`](super::SinkExt::with_flat_map) method.
#[must_use = "sinks do nothing unless polled"]
pub struct WithFlatMap<Si, Item, U, St, F> {
    sink: Si,
    f: F,
    stream: Option<St>,
    buffer: Option<Item>,
    _marker: PhantomData<fn(U)>,
}

impl<Si: Unpin, Item, U, St: Unpin, F> Unpin for WithFlatMap<Si, Item, U, St, F> {}

impl<Si, Item, U, St, F> fmt::Debug for WithFlatMap<Si, Item, U, St, F>
where
    Si: fmt::Debug,
    St: fmt::Debug,
    Item: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WithFlatMap")
            .field("sink", &self.sink)
            .field("stream", &self.stream)
            .field("buffer", &self.buffer)
            .finish()
    }
}

impl<Si, Item, U, St, F> WithFlatMap<Si, Item, U, St, F>
where
    Si: Sink<Item>,
    F: FnMut(U) -> St,
    St: Stream<Item = Result<Item, Si::Error>>,
{
    pub(super) fn new(sink: Si, f: F) -> Self {
        Self {
            sink,
            f,
            stream: None,
            buffer: None,
            _marker: PhantomData,
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
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) }
    }

    /// Consumes this combinator, returning the underlying sink.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> Si {
        self.sink
    }

    fn try_empty_stream(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Si::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut sink = unsafe { Pin::new_unchecked(&mut this.sink) };

        if this.buffer.is_some() {
            match sink.as_mut().poll_ready(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
            let item = this.buffer.take().unwrap();
            if let Err(e) = sink.as_mut().start_send(item) {
                return Poll::Ready(Err(e));
            }
        }
        let stream_pin = unsafe { Pin::new_unchecked(&mut this.stream) };
        if let Some(mut some_stream) = stream_pin.as_pin_mut() {
            loop {
                let item = match some_stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(item))) => Some(item),
                    Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                    Poll::Ready(None) => None,
                    Poll::Pending => return Poll::Pending,
                };

                if let Some(item) = item {
                    match sink.as_mut().poll_ready(cx) {
                        Poll::Ready(Ok(())) => {
                            if let Err(e) = sink.as_mut().start_send(item) {
                                return Poll::Ready(Err(e));
                            }
                        }
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => {
                            this.buffer = Some(item);
                            return Poll::Pending;
                        }
                    };
                } else {
                    break;
                }
            }
        }
        this.stream = None;
        Poll::Ready(Ok(()))
    }
}

// Forwarding impl of Stream from the underlying sink
impl<S, Item, U, St, F> Stream for WithFlatMap<S, Item, U, St, F>
where
    S: Stream + Sink<Item>,
    F: FnMut(U) -> St,
    St: Stream<Item = Result<Item, S::Error>>,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) }.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sink.size_hint()
    }
}

impl<Si, Item, U, St, F> Sink<U> for WithFlatMap<Si, Item, U, St, F>
where
    Si: Sink<Item>,
    F: FnMut(U) -> St,
    St: Stream<Item = Result<Item, Si::Error>>,
{
    type Error = Si::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.try_empty_stream(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: U) -> Result<(), Self::Error> {
        let this = unsafe { self.get_unchecked_mut() };

        assert!(this.stream.is_none());
        this.stream = Some((this.f)(item));
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().try_empty_stream(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        };
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) }.poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.as_mut().try_empty_stream(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        };
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().sink) }.poll_close(cx)
    }
}
