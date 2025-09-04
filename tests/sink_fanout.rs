#[cfg(feature = "sync")]
#[tokio::test]
async fn it_works() {
    use async_sink::{sync::mpsc, SinkExt};
    use tokio::join;
    use tokio_stream::{self as stream, StreamExt};

    let (tx1, rx1) = mpsc::channel(1);
    let (tx2, rx2) = mpsc::channel(2);
    let tx = tx1.fanout(tx2).sink_map_err(|_| ());

    let src = stream::iter((0..10).map(Ok));
    let fwd = src.forward(tx);

    let collect_fut1 = rx1.collect::<Vec<_>>();
    let collect_fut2 = rx2.collect::<Vec<_>>();
    let (_, vec1, vec2) = join!(fwd, collect_fut1, collect_fut2).await;

    let expected = (0..10).map(Ok::<_, ()>).collect::<Vec<_>>();

    assert_eq!(vec1, expected);
    assert_eq!(vec2, expected);
}
