// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

use pin_project::pin_project;
use tokio::time::{Instant, Sleep, sleep};
use tokio_stream::Stream;

/// Yields `make_keepalive()` whenever the inner stream has been idle for `interval`.
///
/// Ends when the inner stream ends. The stream is fused even if the inner stream is not.
///
/// Used to keep alive h2 streams through proxies that close idle streams. HTTP/2 PINGs don't help
/// there: they only keep the connection to nearest proxy alive.
//
// Implementation detail: no jitter -- keepalive streams created together should fire together, so
// their keepalives share one h2 connection write (and one radio wake-up on mobile).
#[pin_project]
pub struct KeepAliveStream<S, F> {
    #[pin]
    inner: S,
    #[pin]
    sleep: Sleep,
    interval: Duration,
    make_keepalive: F,
    /// Makes this stream fused without assuming that the underlying stream is fused.
    done: bool,
}

impl<S, F> KeepAliveStream<S, F> {
    /// # Panics
    ///
    /// Requires a running tokio runtime with support for time.
    pub fn new(inner: S, interval: Duration, make_keepalive: F) -> Self {
        Self {
            inner,
            sleep: sleep(interval),
            interval,
            make_keepalive,
            done: false,
        }
    }
}

impl<S, F> Stream for KeepAliveStream<S, F>
where
    S: Stream,
    F: FnMut() -> S::Item,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if *this.done {
            return Poll::Ready(None);
        }
        match this.inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                this.sleep.as_mut().reset(Instant::now() + *this.interval);
                return Poll::Ready(Some(item));
            }
            Poll::Ready(None) => {
                *this.done = true;
                return Poll::Ready(None);
            }
            Poll::Pending => {}
        }
        ready!(this.sleep.as_mut().poll(cx));
        this.sleep.as_mut().reset(Instant::now() + *this.interval);
        Poll::Ready(Some((this.make_keepalive)()))
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use super::*;

    use futures_util::FutureExt;
    use tokio::{sync::mpsc, time::advance};
    use tokio_stream::{StreamExt, wrappers::ReceiverStream};

    const INTERVAL: Duration = Duration::from_secs(30);

    fn keepalive_stream(
        rx: mpsc::Receiver<u32>,
    ) -> KeepAliveStream<ReceiverStream<u32>, impl FnMut() -> u32> {
        KeepAliveStream::new(ReceiverStream::new(rx), INTERVAL, || 0)
    }

    #[tokio::test(start_paused = true)]
    async fn passes_items_through() {
        let (tx, rx) = mpsc::channel(10);
        let mut stream = pin!(keepalive_stream(rx));

        for i in 1..=3 {
            advance(INTERVAL / 2).await;
            tx.send(i).await.unwrap();
            assert_eq!(stream.next().await, Some(i));
        }
        assert_eq!(stream.next().now_or_never(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn yields_keepalive_periodically_when_idle() {
        let start = Instant::now();
        let (_tx, rx) = mpsc::channel(10);
        let mut stream = pin!(keepalive_stream(rx));

        advance(INTERVAL - Duration::from_millis(1)).await;
        assert_eq!(stream.next().now_or_never(), None);

        advance(Duration::from_millis(1)).await;
        assert_eq!(stream.next().await, Some(0));
        assert_eq!(Instant::now(), start + INTERVAL);

        advance(INTERVAL - Duration::from_millis(1)).await;
        assert_eq!(stream.next().now_or_never(), None);

        advance(Duration::from_millis(1)).await;
        assert_eq!(stream.next().await, Some(0));
        assert_eq!(Instant::now(), start + 2 * INTERVAL);
    }

    #[tokio::test(start_paused = true)]
    async fn item_resets_timer() {
        let start = Instant::now();
        let (tx, rx) = mpsc::channel(10);
        let mut stream = pin!(keepalive_stream(rx));

        let one_sec = Duration::from_secs(1);
        advance(INTERVAL - one_sec).await;
        tx.send(1).await.unwrap();
        assert_eq!(stream.next().await, Some(1));

        // The original deadline passes without a keepalive
        advance(one_sec).await;
        assert_eq!(stream.next().now_or_never(), None);

        advance(INTERVAL - one_sec).await;
        assert_eq!(stream.next().await, Some(0));
        assert_eq!(Instant::now(), start + 2 * INTERVAL - one_sec);
    }

    #[tokio::test(start_paused = true)]
    async fn item_wins_over_expired_timer() {
        let (tx, rx) = mpsc::channel(10);
        let mut stream = pin!(keepalive_stream(rx));

        advance(INTERVAL).await;
        tx.send(1).await.unwrap();

        assert_eq!(stream.next().await, Some(1));
        assert_eq!(stream.next().now_or_never(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn ends_with_inner_and_stays_ended() {
        let (tx, rx) = mpsc::channel(10);
        let mut stream = pin!(keepalive_stream(rx));

        advance(INTERVAL).await;
        drop(tx);
        assert_eq!(stream.next().await, None);

        advance(INTERVAL).await;
        assert_eq!(stream.next().await, None);
    }
}
