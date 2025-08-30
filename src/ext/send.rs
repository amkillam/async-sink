use super::Feed;
use super::Sink;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Future for the [`send`](super::SinkExt::send) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Send<'a, Si: ?Sized, Item> {
    feed: Feed<'a, Si, Item>,
}

// `Send` is `Unpin` because it only contains a `Feed` which is `Unpin`.
impl<Si: ?Sized, Item> Unpin for Send<'_, Si, Item> {}

impl<'a, Si: Sink<Item> + Unpin + ?Sized, Item> Send<'a, Si, Item> {
    pub(super) fn new(sink: &'a mut Si, item: Item) -> Self {
        Self {
            feed: Feed::new(sink, item),
        }
    }
}

impl<Si: Sink<Item> + Unpin + ?Sized, Item> Future for Send<'_, Si, Item> {
    type Output = Result<(), Si::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        if this.feed.is_item_pending() {
            match Pin::new(&mut this.feed).poll(cx) {
                Poll::Ready(Ok(())) => {
                    debug_assert!(!this.feed.is_item_pending());
                    // Fall through to flushing.
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        // We're done sending the item, but want to block on flushing the sink.
        this.feed.sink_pin_mut().poll_flush(cx)
    }
}
