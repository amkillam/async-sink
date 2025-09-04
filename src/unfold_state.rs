use core::marker::PhantomPinned;
use core::mem;
use core::pin::Pin;

/// Projection type for `UnfoldState` when projecting by reference.
/// This is manually generated equivalent of `#[project = UnfoldStateProj]` from `pin_project_lite`.
pub(crate) enum UnfoldStateProj<'pin, T, R> {
    #[allow(dead_code)]
    Value {
        value: &'pin mut T,
    },
    Future {
        future: Pin<&'pin mut R>,
    },
    Empty,
}

/// Projection type for `UnfoldState` when replacing a variant and taking ownership of its fields.
/// This is manually generated equivalent of `#[project_replace = UnfoldStateProjReplace]` from `pin_project_lite`.
pub(crate) enum UnfoldStateProjReplace<T, R> {
    Value {
        value: T,
    },
    #[allow(dead_code)]
    Future {
        future: R,
    },
    Empty,
}

/// UnfoldState used for stream and sink unfolds
#[derive(Debug)]
pub(crate) enum UnfoldState<T, R> {
    Value {
        value: T,
    },
    Future {
        future: R,
        // This `PhantomPinned` field makes the `Future` variant `!Unpin`.
        // This is how `pin_project_lite` ensures that types containing `#[pin]` fields
        // are `!Unpin` by default, allowing for safe pin-projection.
        _pin: PhantomPinned,
    },
    Empty,
}

impl<T, R> UnfoldState<T, R> {
    /// Manually implemented `project` method, equivalent to what `pin_project_lite` generates.
    /// It takes a `Pin<&mut Self>` and returns a projection enum `UnfoldStateProj`.
    pub(crate) fn project(self: Pin<&mut Self>) -> UnfoldStateProj<'_, T, R> {
        // Safety: This is safe because we are projecting a pinned reference to its fields.
        // The `future` field is conceptually pinned (due to `_pin: PhantomPinned` in its variant),
        // so we can safely create a `Pin<&mut R>` from `&mut R` as long as `self` is pinned.
        // `get_unchecked_mut` is safe because we are not moving `self` and only taking
        // mutable references to its fields.
        unsafe {
            match self.get_unchecked_mut() {
                UnfoldState::Value { value } => UnfoldStateProj::Value { value },
                UnfoldState::Future { future, _pin } => UnfoldStateProj::Future {
                    future: Pin::new_unchecked(future),
                },
                UnfoldState::Empty => UnfoldStateProj::Empty,
            }
        }
    }

    /// Manually implemented `project_replace` method, equivalent to what `pin_project_lite` generates.
    /// It takes a `Pin<&mut Self>` and a new `Self` value, replaces the current variant,
    /// and returns the old variant's contents as `UnfoldStateProjReplace`.
    pub(crate) fn project_replace(self: Pin<&mut Self>, new: Self) -> UnfoldStateProjReplace<T, R> {
        // Safety: This is safe because we are replacing the entire enum variant with a new one,
        // and then returning the old variant's contents. `get_unchecked_mut` is safe because
        // we are not moving `self`, and `mem::replace` handles the replacement atomically.
        unsafe {
            let old = mem::replace(self.get_unchecked_mut(), new);
            match old {
                UnfoldState::Value { value } => UnfoldStateProjReplace::Value { value },
                UnfoldState::Future { future, _pin } => UnfoldStateProjReplace::Future { future },
                UnfoldState::Empty => UnfoldStateProjReplace::Empty,
            }
        }
    }

    pub(crate) fn project_future(self: Pin<&mut Self>) -> Option<Pin<&mut R>> {
        match self.project() {
            UnfoldStateProj::Future { future } => Some(future),
            _ => None,
        }
    }

    pub(crate) fn take_value(self: Pin<&mut Self>) -> Option<T> {
        match &*self {
            Self::Value { .. } => {
                // If the current state is `Value`, we replace it with `Empty` and take the value.
                // The `unreachable!` is safe because we've matched `Self::Value` above,
                // so `project_replace` must return `UnfoldStateProjReplace::Value`.
                match self.project_replace(Self::Empty) {
                    UnfoldStateProjReplace::Value { value } => Some(value),
                    _ => unreachable!(),
                }
            }
            _ => None,
        }
    }
}

// Manually implemented `Unpin` for `UnfoldState`.
// `pin_project_lite` generates an `Unpin` implementation that makes the type `Unpin`
// if all *unpinned* fields are `Unpin`. The `#[pin]` field (`future: R`) does not
// require `R` to be `Unpin` for `UnfoldState` to be `Unpin`.
// Therefore, `UnfoldState` is `Unpin` if `T` is `Unpin`.
impl<T: Unpin, R> Unpin for UnfoldState<T, R> {}
