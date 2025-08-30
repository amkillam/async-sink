use core::pin::Pin;

/// UnfoldState used for stream and sink unfolds
#[derive(Debug)]
pub(crate) enum UnfoldState<T, Fut> {
    Value {
        value: T,
    },
    Future {
        future: Fut,
    },
    Empty,
}

/// Projection enum for `project()`
pub(crate) enum UnfoldStateProj<'a, T, Fut> {
    Value {
        value: &'a mut T,
    },
    Future {
        future: Pin<&'a mut Fut>,
    },
    Empty,
}

/// Projection enum for `project_replace()`
pub(crate) enum UnfoldStateProjReplace<T> {
    Value {
        value: T,
    },
    Future,
    Empty,
}

impl<T, Fut> UnfoldState<T, Fut> {
    /// Create projection for pinned &mut self
    fn project<'a>(self: Pin<&'a mut Self>) -> UnfoldStateProj<'a, T, Fut> {
        // SAFETY: pin projection guarantees that `future` remains pinned.
        unsafe {
            match self.get_unchecked_mut() {
                UnfoldState::Value { value } => UnfoldStateProj::Value { value },
                UnfoldState::Future { future } => UnfoldStateProj::Future {
                    future: Pin::new_unchecked(future),
                },
                UnfoldState::Empty => UnfoldStateProj::Empty,
            }
        }
    }

    /// Replace enum variant and return owned fields of previous value
    fn project_replace(self: Pin<&mut Self>, replacement: Self) -> UnfoldStateProjReplace<T> {
        unsafe {
            let this = self.get_unchecked_mut();
            let old = core::mem::replace(this, replacement);
            match old {
                UnfoldState::Value { value } => UnfoldStateProjReplace::Value { value },
                UnfoldState::Future { .. } => UnfoldStateProjReplace::Future,
                UnfoldState::Empty => UnfoldStateProjReplace::Empty,
            }
        }
    }
}

impl<T, Fut> UnfoldState<T, Fut> {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub(crate) fn is_future(&self) -> bool {
        matches!(self, Self::Future { .. })
    }

    pub(crate) fn project_future(self: Pin<&mut Self>) -> Option<Pin<&mut Fut>> {
        match self.project() {
            UnfoldStateProj::Future { future } => Some(future),
            _ => None,
        }
    }

    pub(crate) fn take_value(self: Pin<&mut Self>) -> Option<T> {
        match &*self {
            Self::Value { .. } => match self.project_replace(Self::Empty) {
                UnfoldStateProjReplace::Value { value } => Some(value),
                _ => unreachable!(),
            },
            _ => None,
        }
    }
}
