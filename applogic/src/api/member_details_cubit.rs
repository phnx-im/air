// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use aircoreclient::{
    ChatId,
    clients::CoreUser,
    store::{Store, StoreEntityId},
};
use flutter_rust_bridge::frb;
use mimi_room_policy::{MimiProposal, RoleIndex, VerifiedRoomState};
use tls_codec::Serialize;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tracing::error;

use crate::{
    StreamSink,
    api::{types::UiUserId, user_cubit::UserCubitBase},
    util::{Cubit, CubitCore, spawn_from_sync},
};

#[frb(dart_metadata = ("freezed"))]
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash)]
pub struct MemberDetailsState {
    pub members: Vec<UiUserId>,
    pub room_state: Option<UiRoomState>,
}

#[frb(dart_metadata = ("freezed"))]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UiRoomState {
    our_user: UserId,
    state: VerifiedRoomState,
}

impl UiRoomState {
    #[frb(sync)]
    pub fn can_kick(&self, target: &UiUserId) -> bool {
        let Ok(user) = self.our_user.tls_serialize_detached() else {
            return false;
        };
        let Ok(target) = UserId::from(target.clone()).tls_serialize_detached() else {
            return false;
        };

        self.state
            .can_apply_regular_proposals(
                &user,
                &[MimiProposal::ChangeRole {
                    target,
                    role: RoleIndex::Outsider,
                }],
            )
            .is_ok()
    }
}

#[frb(opaque)]
pub struct MemberDetailsCubitBase {
    core: CubitCore<MemberDetailsState>,
}

impl MemberDetailsCubitBase {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase, chat_id: ChatId) -> Self {
        let store = user_cubit.core_user().clone();

        let core = CubitCore::new();

        let context = MemberDetailsContext {
            store: store.clone(),
            state_tx: core.state_tx().clone(),
            chat_id,
        };

        let load_initial_state_task =
            core.cancellation_token()
                .clone()
                .run_until_cancelled_owned({
                    let context = context.clone();
                    async move { context.load_and_emit_state().await }
                });
        spawn_from_sync(load_initial_state_task);

        let update_state_task = core
            .cancellation_token()
            .clone()
            .run_until_cancelled_owned(context.update_state_task());
        spawn_from_sync(update_state_task);

        Self { core }
    }

    // Cubit interface

    pub fn close(&mut self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> MemberDetailsState {
        self.core.state()
    }

    pub async fn stream(&mut self, sink: StreamSink<MemberDetailsState>) {
        self.core.stream(sink).await;
    }
}

#[frb(ignore)]
#[derive(Clone)]
struct MemberDetailsContext {
    store: CoreUser,
    state_tx: watch::Sender<MemberDetailsState>,
    chat_id: ChatId,
}

impl MemberDetailsContext {
    async fn load_and_emit_state(&self) {
        let members = self
            .store
            .chat_participants(self.chat_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(From::from)
            .collect();

        let room_state = self
            .store
            .load_room_state(&self.chat_id)
            .await
            .inspect_err(|error| error!(%error, "Failed to load room state"))
            .map(|(our_user, state)| UiRoomState { our_user, state })
            .ok();

        let _ = self.state_tx.send(MemberDetailsState {
            members,
            room_state,
        });
    }

    async fn update_state_task(self) {
        let mut notifications = self.store.subscribe();
        while let Some(notification) = notifications.next().await {
            // If this chat has changed, or any user changed
            if notification.ops.contains_key(&self.chat_id.into())
                || notification
                    .ops
                    .keys()
                    .any(|entity_id| matches!(entity_id, StoreEntityId::User(_)))
            {
                self.load_and_emit_state().await;
            }
        }
    }
}
