use super::Sink;
use crate::unfold_state::UnfoldState;
use core::{
    fmt,
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    task::{Context, Poll},
};

/// Sink for the [`unfold`] function.
#[must_use = "sinks do nothing unless polled"]
pub struct Unfold<T, F, Fut> {
    function: F,
    state: UnfoldState<T, Fut>,
}

struct UnfoldProj<'pin, T, F, Fut> {
    function: &'pin mut F,
    state: Pin<&'pin mut UnfoldState<T, Fut>>,
}

impl<T, F, Fut> Unfold<T, F, Fut> {
    // Helper to get a mutable reference to the `function` field and a
    // pinned mutable reference to the `state` field.
    //
    // # Safety
    //
    // This is `unsafe` because it returns a `Pin` to one of the fields of the
    // struct. The caller must ensure that they don't move the struct while this
    // `Pin` is in use.
    unsafe fn project<'a>(self: Pin<&'a mut Self>) -> UnfoldProj<'a, T, F, Fut> {
        let this = self.get_unchecked_mut();
        UnfoldProj {
            function: &mut this.function,
            state: Pin::new_unchecked(&mut this.state),
        }
    }
}

impl<T, F, Fut> fmt::Debug for Unfold<T, F, Fut>
where
    T: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Unfold")
            .field("state", &self.state)
            .finish()
    }
}

/// Create a sink from a function which processes one item at a time.
///
/// # Examples
///
/// ```
/// use core::pin::pin;
/// use async_sink::SinkExt;
/// use tokio::sync::Mutex;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
/// let output: Arc<Mutex<Vec<usize>>> = Arc::new(tokio::sync::Mutex::new(Vec::new()));
///
/// let unfold = async_sink::unfold(0, |mut sum, i: usize| {
///    let cb_output = output.clone();
///    async move {
///         sum += i;
///         cb_output.clone().lock().await.push(sum);
///         Ok::<_, core::convert::Infallible>(sum)
///    }
/// });
/// let mut unfold = pin!(unfold);
/// let input: [usize; 3] = [5, 15, 35];
/// assert!(unfold.send_all(&mut tokio_stream::iter(input.iter().copied().map(|i| Ok(i)))).await.is_ok());
/// assert_eq!(output.lock().await.as_slice(),input.iter().scan(0, |state, &x|
///   { *state += x; Some(*state) }).collect::<Vec<usize>>().as_slice()
/// );
/// }
/// ```
pub fn unfold<T, F, Fut, Item, E>(init: T, function: F) -> Unfold<T, F, Fut>
where
    F: FnMut(T, Item) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    Unfold {
        function,
        state: UnfoldState::Value { value: init },
    }
}

impl<T, F, Fut, Item, E> Sink<Item> for Unfold<T, F, Fut>
where
    F: FnMut(T, Item) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    type Error = E;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let mut this = unsafe { self.project() };
        let future = match this.state.as_mut().take_value() {
            Some(value) => (this.function)(value, item),
            None => panic!("start_send called without poll_ready being called first"),
        };
        this.state.set(UnfoldState::Future {
            future,
            _pin: PhantomPinned,
        });
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut this = unsafe { self.project() };
        if let Some(future) = this.state.as_mut().project_future() {
            match future.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok(state)) => {
                    this.state.set(UnfoldState::Value { value: state });
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(err)) => {
                    this.state.set(UnfoldState::Empty);
                    Poll::Ready(Err(err))
                }
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}
