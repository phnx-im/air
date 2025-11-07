// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

use futures_util::Stream;
use pin_project::pin_project;
use tokio_util::sync::{CancellationToken, DropGuard, WaitForCancellationFutureOwned};

/// A stream that is cancellable.
///
/// The stream will yield items from the underlying stream until the provided cancellation token is
/// cancelled or the underlying stream ends.
///
/// Note: Cancellation wakes up the task polling the stream (if any), that is, cancellation is
/// **not** lazy in the sense that the underlaying stream first has to yield an item to observe
/// cancellation.
#[pin_project]
pub(crate) struct CancellableStream<S> {
    #[pin]
    stream: S,
    #[pin]
    cancel_fut: Option<WaitForCancellationFutureOwned>,
}

impl<S> CancellableStream<S> {
    pub(crate) fn new(stream: S, cancel: CancellationToken) -> Self {
        Self {
            stream,
            cancel_fut: Some(cancel.cancelled_owned()),
        }
    }
}

impl<S: Stream + Unpin> Stream for CancellableStream<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // Poll the cancellation future
        match this.cancel_fut.as_mut().as_pin_mut() {
            Some(cancel_fut) => {
                if cancel_fut.poll(cx).is_ready() {
                    // Cancellation fired, drop the future
                    this.cancel_fut.set(None);
                    return Poll::Ready(None);
                }
            }
            None => {
                // The future has been dropped, so we're done
                return Poll::Ready(None);
            }
        }

        this.stream.poll_next(cx)
    }
}

/// A streams that cancels the provided cancellation token when the underlying stream ends.
///
/// The token is cancelled when the underlying stream ends, or when the stream is dropped.
///
/// This is useful when you want to bind a lifetime of some resource to the lifetime of the stream.
/// E.g. use with [`CancellableStream`] to create a dependency between two streams.
#[pin_project]
pub(crate) struct CancellingStream<S> {
    #[pin]
    stream: S,
    cancel: Option<DropGuard>,
}

impl<S> CancellingStream<S> {
    pub(crate) fn new(stream: S, cancel: CancellationToken) -> Self {
        Self {
            stream,
            cancel: Some(cancel.drop_guard()),
        }
    }
}

impl<S: Stream> Stream for CancellingStream<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        let item = ready!(this.stream.poll_next(cx));
        if item.is_none() {
            // The stream has been closed, drop the guard
            this.cancel.take();
        }
        Poll::Ready(item)
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use super::*;

    use tokio::sync::mpsc;
    use tokio_stream::{StreamExt, wrappers::ReceiverStream};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn cancellable_stream() {
        let cancel_token = CancellationToken::new();

        let (tx, rx) = mpsc::channel(10);

        let mut cancellable_stream = pin!(CancellableStream::new(
            ReceiverStream::new(rx),
            cancel_token.clone()
        ));

        tx.send(42).await.unwrap();

        let item = cancellable_stream.next().await;
        assert_eq!(item, Some(42));

        cancel_token.cancel();

        tx.send(99).await.unwrap();

        let item_after_cancel = cancellable_stream.next().await;
        assert_eq!(item_after_cancel, None);
    }

    #[tokio::test]
    async fn cancellable_stream_exhaustion_without_cancellation() {
        let cancel_token = CancellationToken::new();
        let (tx, rx) = mpsc::channel(2);

        let cancellable_stream = pin!(CancellableStream::new(
            ReceiverStream::new(rx),
            cancel_token.clone()
        ));

        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        drop(tx); // Close the channel

        let items: Vec<_> = cancellable_stream.collect().await;
        assert_eq!(items, vec![1, 2]);
    }

    #[tokio::test]
    async fn cancelling_stream_yields_items() {
        let stream = tokio_stream::iter([1, 2, 3]);
        let token = CancellationToken::new();
        let mut cancel_stream = CancellingStream::new(stream, token.clone());

        let mut collected = Vec::new();
        while let Some(item) = cancel_stream.next().await {
            collected.push(item);
        }

        assert_eq!(collected, vec![1, 2, 3]);
        assert!(token.is_cancelled());
        assert!(cancel_stream.cancel.is_none());
    }

    #[tokio::test]
    async fn cancelling_stream_cancels_when_stream_ends() {
        let stream = tokio_stream::empty::<i32>();
        let token = CancellationToken::new();
        let mut cancel_stream = CancellingStream::new(stream, token.clone());

        // Poll the stream to completion
        assert!(cancel_stream.next().await.is_none());

        // Token should be cancelled and guard should be dropped
        assert!(token.is_cancelled());
        assert!(cancel_stream.cancel.is_none());
    }

    #[tokio::test]
    async fn cancelling_stream_does_not_cancel_early() {
        let stream = tokio_stream::iter([1, 2]);
        let token = CancellationToken::new();
        let mut cancel_stream = CancellingStream::new(stream, token.clone());

        // Poll the first item
        assert_eq!(cancel_stream.next().await, Some(1));

        assert!(!token.is_cancelled());
        assert!(cancel_stream.cancel.is_some());

        assert_eq!(cancel_stream.next().await, Some(2));
    }
}
