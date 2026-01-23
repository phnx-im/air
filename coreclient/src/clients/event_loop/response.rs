// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Support for sending responses from message passing event loop interfaces.

use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

use anyhow::anyhow;
use pin_project::pin_project;
use tokio::sync::oneshot;
use tracing::warn;

/// Creates a new [`Responder`] and [`Response`].
///
/// Can be only created in the `event_loop` submodule.
pub(super) fn responder<T, E>() -> (Responder<T, E>, Response<T, E>) {
    let (tx, rx) = oneshot::channel();
    (Responder { tx: Some(tx) }, Response { rx })
}

#[pin_project]
pub(super) struct Response<T, E> {
    #[pin]
    rx: oneshot::Receiver<Result<T, ResponderError<E>>>,
}

impl<T, E> Future for Response<T, E> {
    type Output = Result<T, ResponderError<E>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match ready!(self.project().rx.poll(cx)) {
            Ok(res) => Poll::Ready(res),
            Err(_) => Poll::Ready(Err(ResponderError::Cancelled)),
        }
    }
}

pub(super) struct Responder<T, E> {
    tx: Option<oneshot::Sender<Result<T, ResponderError<E>>>>,
}

impl<T, E> Responder<T, E> {
    pub(super) fn send(mut self, response: Result<T, ResponderError<E>>) {
        if let Some(tx) = self.tx.take()
            && tx.send(response).is_err()
        {
            warn!("responder receiver dropped without response");
        }
    }
}

#[derive(Debug)]
pub(super) enum ResponderError<E> {
    Cancelled,
    Fatal(anyhow::Error),
    #[expect(unused)]
    Error(E),
}

impl<E: std::error::Error + Send + Sync + 'static> From<ResponderError<E>> for anyhow::Error {
    fn from(error: ResponderError<E>) -> Self {
        match error {
            ResponderError::Cancelled => anyhow!("responder cancelled"),
            ResponderError::Fatal(error) => error,
            ResponderError::Error(error) => error.into(),
        }
    }
}

impl<T, E> Drop for Responder<T, E> {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take()
            && tx.send(Err(ResponderError::Cancelled)).is_err()
        {
            warn!("responder receiver dropped without response");
        }
    }
}
