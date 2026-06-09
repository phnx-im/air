// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airbackend::{
    messages::intra_backend::DsFanOutMessage,
    qs::{
        PushNotificationProvider, Qs, QsConnector, errors::QsEnqueueError,
        network_provider::NetworkProvider,
    },
};
use aircommon::{crypto::signatures::keys::QsUserVerifyingKey, identifiers::QsUserId};

#[derive(Debug, Clone)]
pub struct SimpleEnqueueProvider<N: NetworkProvider, P: PushNotificationProvider> {
    pub qs: Qs,
    pub push_notification_provider: P,
    pub network: N,
}

impl<N, P> QsConnector for SimpleEnqueueProvider<N, P>
where
    N: NetworkProvider + Clone,
    P: PushNotificationProvider + Clone,
{
    type EnqueueError = QsEnqueueError<N>;

    fn dispatch(
        &self,
        message: DsFanOutMessage,
    ) -> impl Future<Output = Result<(), Self::EnqueueError>> + Send + 'static {
        let provider = self.clone();
        async move {
            provider
                .qs
                .enqueue_message(
                    &provider.push_notification_provider,
                    &provider.network,
                    message,
                )
                .await
        }
    }

    fn user_verifying_key(
        &self,
        qs_user_id: QsUserId,
    ) -> impl Future<Output = Result<Option<QsUserVerifyingKey>, Self::EnqueueError>> + Send + 'static
    {
        let provider = self.clone();
        async move {
            provider
                .qs
                .load_user_verifying_key(&qs_user_id)
                .await
                .map_err(|_| QsEnqueueError::StorageError)
        }
    }
}
