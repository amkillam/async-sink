use super::Sink;
use core::{
    fmt,
    future::Future,
    ops::DerefMut,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_stream::Stream;

/// Future for the [`send_all`](super::SinkExt::send_all) method.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SendAll<'a, Si, Item, St>
where
    Si: ?Sized + Sink<Item>,
    St: Stream<Item = Result<Item, Si::Error>> + ?Sized,
{
    sink: &'a mut Si,
    stream: &'a mut St,
    buffered: Option<Item>,
    stream_done: bool,
}

impl<Si, Item, St> fmt::Debug for SendAll<'_, Si, Item, St>
where
    Si: fmt::Debug + ?Sized + Sink<Item>,
    St: fmt::Debug + Stream<Item = Result<Item, Si::Error>> + ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SendAll")
            .field("sink", &self.sink)
            .field("stream", &self.stream)
            .field("buffered", &self.buffered)
            .field("stream_done", &self.stream_done)
            .finish()
    }
}

impl<'a, Si, Item, St> SendAll<'a, Si, Item, St>
where
    Si: Sink<Item> + Unpin + ?Sized,
    Si::Error: core::error::Error,
    St: Stream<Item = Result<Item, Si::Error>> + Unpin + ?Sized,
{
    pub(super) fn new(sink: &'a mut Si, stream: &'a mut St) -> Self {
        Self {
            sink,
            stream,
            buffered: None,
            stream_done: false,
        }
    }

    fn try_start_send(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        item: Item,
    ) -> Poll<Result<(), Si::Error>> {
        let this = unsafe { Pin::get_unchecked_mut(self) };
        debug_assert!(this.buffered.is_none());
        match Pin::new(&mut *this.sink).poll_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Pin::new(&mut *this.sink).start_send(item)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => {
                this.buffered = Some(item);
                Poll::Pending
            }
        }
    }
}

impl<'a, Si, Item, St> Future for SendAll<'a, Si, Item, St>
where
    Si: Sink<Item> + Unpin + ?Sized,
    St: Stream<Item = Result<Item, Si::Error>> + Unpin + ?Sized,
{
    type Output = Result<(), Si::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(item) = unsafe { self.as_mut().get_unchecked_mut() }.buffered.take() {
            match self.as_mut().try_start_send(cx, item) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        loop {
            let this = unsafe { self.as_mut().get_unchecked_mut() };
            if this.stream_done {
                return Pin::new(&mut *this.sink).poll_flush(cx);
            }

            match <St as Stream>::poll_next(Pin::new(this.stream.deref_mut()), cx) {
                Poll::Ready(Some(Ok(item))) => match self.as_mut().try_start_send(cx, item) {
                    Poll::Ready(Ok(())) => continue,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                },
                Poll::Ready(Some(Err(e))) => {
                    unsafe { self.as_mut().get_unchecked_mut() }.stream_done = true;
                    return Poll::Ready(Err(e));
                }
                Poll::Ready(None) => {
                    unsafe { self.as_mut().get_unchecked_mut() }.stream_done = true;
                }
                Poll::Pending => {
                    let this = unsafe { self.as_mut().get_unchecked_mut() };
                    return match Pin::new(&mut *this.sink).poll_flush(cx) {
                        Poll::Ready(Ok(())) => Poll::Pending,
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => Poll::Pending,
                    };
                }
            }
        }
    }
}
