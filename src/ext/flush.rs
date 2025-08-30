use super::Sink;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Future for the [`flush`](super::SinkExt::flush) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Flush<'a, Si: ?Sized, Item> {
    sink: &'a mut Si,
    _phantom: PhantomData<fn(Item)>,
}

impl<Si: Unpin + ?Sized, Item> Unpin for Flush<'_, Si, Item> {}

impl<'a, Si: Sink<Item> + Unpin + ?Sized, Item> Flush<'a, Si, Item> {
    pub(super) fn new(sink: &'a mut Si) -> Self {
        Self {
            sink,
            _phantom: PhantomData,
        }
    }
}

impl<Si: Sink<Item> + Unpin + ?Sized, Item> Future for Flush<'_, Si, Item> {
    type Output = Result<(), Si::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        Pin::new(&mut *this.sink).poll_flush(cx)
    }
}
