// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::stream::{FusedStream, Stream};
use pin_project::pin_project;

/// Find the first error in the source chain that is of type `T`.
pub(crate) fn find_cause<T: std::error::Error + 'static>(
    error: &dyn std::error::Error,
) -> Option<&T> {
    let mut source = error.source();
    while let Some(error) = source {
        if let Some(typed) = error.downcast_ref() {
            return Some(typed);
        }
        source = error.source();
    }
    None
}

/// Combines two streams into one, terminating when the first stream `a` ends.
///
/// If `a` ends, the resulting stream will attempt to drain all currently
/// available items from `b` before closing. It stops as soon as `b` returns
/// `Poll::Pending`.
pub(crate) fn select_until_first_ends<A, B>(a: A, b: B) -> impl Stream<Item = A::Item>
where
    A: Stream,
    B: Stream<Item = A::Item>,
{
    SelectUntilFirstEnds {
        a,
        b,
        a_done: false,
        b_done: false,
        poll_next: SelectUntilFirstPollNext::A,
    }
}

#[pin_project]
struct SelectUntilFirstEnds<A, B> {
    #[pin]
    a: A,
    #[pin]
    b: B,
    a_done: bool,
    b_done: bool,
    poll_next: SelectUntilFirstPollNext,
}

#[derive(Clone, Copy)]
enum SelectUntilFirstPollNext {
    A,
    B,
}

impl SelectUntilFirstPollNext {
    fn flip(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

impl<A, B> SelectUntilFirstEnds<A, B>
where
    A: Stream,
    B: Stream<Item = A::Item>,
{
    fn poll_drain(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<A::Item>> {
        let this = self.project();

        if *this.b_done {
            return Poll::Ready(None);
        }

        match this.b.poll_next(cx) {
            Poll::Ready(Some(item)) => Poll::Ready(Some(item)),
            Poll::Ready(None) | Poll::Pending => {
                // Once A is done, any break in B's availability shuts down the stream.
                *this.b_done = true;
                Poll::Ready(None)
            }
        }
    }

    fn poll_a(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<A::Item>> {
        let this = self.project();
        match this.a.poll_next(cx) {
            Poll::Ready(None) => {
                *this.a_done = true;
                Poll::Ready(None)
            }
            res => res,
        }
    }

    fn poll_b(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<B::Item>> {
        let this = self.project();
        if *this.b_done {
            return Poll::Pending;
        }

        match this.b.poll_next(cx) {
            Poll::Ready(None) => {
                *this.b_done = true;
                Poll::Pending // B ending doesn't end the stream
            }
            res => res,
        }
    }
}

impl<A, B> Stream for SelectUntilFirstEnds<A, B>
where
    A: Stream,
    B: Stream<Item = A::Item>,
{
    type Item = A::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If A is done, we drain B before closing.
        if self.a_done {
            return self.poll_drain(cx);
        }

        // Update the fairness toggle.
        let this = self.as_mut().project();
        let poll_next = mem::replace(this.poll_next, this.poll_next.flip());

        match poll_next {
            SelectUntilFirstPollNext::A => {
                if let Poll::Ready(item) = self.as_mut().poll_a(cx) {
                    return match item {
                        Some(item) => Poll::Ready(Some(item)),
                        None => self.poll_drain(cx),
                    };
                }
                if let Poll::Ready(item) = self.poll_b(cx) {
                    return Poll::Ready(item);
                }
            }
            SelectUntilFirstPollNext::B => {
                if let Poll::Ready(item) = self.as_mut().poll_b(cx) {
                    return Poll::Ready(item);
                }
                if let Poll::Ready(item) = self.as_mut().poll_a(cx) {
                    return match item {
                        Some(item) => Poll::Ready(Some(item)),
                        None => self.poll_drain(cx),
                    };
                }
            }
        }

        Poll::Pending
    }
}

impl<A, B> FusedStream for SelectUntilFirstEnds<A, B>
where
    A: Stream,
    B: Stream<Item = A::Item> + Unpin,
{
    fn is_terminated(&self) -> bool {
        self.a_done
    }
}

#[cfg(test)]
mod tests {
    use std::{future, time::Duration};

    use super::*;

    use futures_util::{
        FutureExt, StreamExt,
        stream::{self, FuturesOrdered},
    };
    use tokio::time;

    #[tokio::test]
    async fn select_until_first_ends_terminates_on_primary_end() {
        let a = stream::iter([1, 2, 3]);
        let b = stream::repeat(99).take(5);

        let combined: Vec<_> = select_until_first_ends(a, b).collect().await;

        assert_eq!(combined, [1, 99, 2, 99, 3, 99, 99, 99]);
    }

    #[tokio::test]
    async fn select_until_first_ends_drains_secondary_until_pending() {
        let a = stream::iter([1, 2, 3]);
        let b: FuturesOrdered<_> = [
            future::ready(99).boxed(),
            future::ready(99).boxed(),
            future::ready(99).boxed(),
            async {
                time::sleep(Duration::from_millis(100)).await;
                99
            }
            .boxed(),
            future::ready(99).boxed(),
            future::ready(99).boxed(),
            future::ready(99).boxed(),
        ]
        .into_iter()
        .collect();

        let combined: Vec<_> = select_until_first_ends(a, b).collect().await;

        assert_eq!(combined, [1, 99, 2, 99, 3, 99]);
    }

    #[tokio::test]
    async fn select_until_first_ends_continues_if_secondary_ends_first() {
        let a = stream::iter([1, 2, 3, 4, 5]);
        let b = stream::iter([99]);

        let combined: Vec<_> = select_until_first_ends(a, b).collect().await;

        assert_eq!(combined, [1, 99, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn select_until_first_ends_fairness_interleaving() {
        let a = stream::iter([1, 3, 5]);
        let b = stream::iter([2, 4, 6]);

        let combined: Vec<_> = select_until_first_ends(a, b).collect().await;

        assert_eq!(combined, [1, 2, 3, 4, 5, 6]);
    }
}
