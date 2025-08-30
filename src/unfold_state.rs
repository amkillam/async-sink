/// UnfoldState used for stream and sink unfolds
#[derive(Debug)]
pub(crate) enum UnfoldState<T, Fut> {
    Value { value: T },
    Future { future: Fut },
    Empty,
}
