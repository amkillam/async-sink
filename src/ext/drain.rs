use super::Sink;
use core::convert::Infallible;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Sink for the [`drain`] function.
#[derive(Debug)]
#[must_use = "sinks do nothing unless polled"]
pub struct Drain<T> {
    marker: PhantomData<T>,
}

/// Create a sink that will just discard all items given to it.
///
/// Similar to [`io::Sink`](::std::io::Sink).
///
/// # Examples
///
/// ```
/// # #[tokio::main]
/// # async fn main() -> Result<(), core::convert::Infallible> {
/// use crate::sink::{self, SinkExt};
///
/// let mut drain = sink::drain();
/// drain.send(5).await?;
/// # Ok(())
/// # }
/// ```
pub fn drain<T>() -> Drain<T> {
    Drain {
        marker: PhantomData,
    }
}

impl<T> Unpin for Drain<T> {}

impl<T> Clone for Drain<T> {
    fn clone(&self) -> Self {
        drain()
    }
}

impl<T> Sink<T> for Drain<T> {
    type Error = Infallible;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, _item: T) -> Result<(), Self::Error> {
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
