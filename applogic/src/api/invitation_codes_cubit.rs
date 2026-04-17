// Re-export for FRB reasons
pub(crate) use aircoreclient::{InvitationCode, RequestInvitationCodeError, TokenId};

use aircoreclient::clients::CoreUser;
use chrono::{DateTime, Utc};
use flutter_rust_bridge::frb;
use tokio::sync::watch::Sender;
use tracing::error;

use crate::{
    StreamSink,
    api::user_cubit::UserCubitBase,
    util::{Cubit, CubitCore, spawn_from_sync},
};

#[doc(hidden)]
#[frb(mirror(InvitationCode))]
#[frb(dart_metadata = ("freezed"))]
pub struct _InvitationCode {
    pub code: String,
    pub copied: bool,
    pub created_at: DateTime<Utc>,
}

/// An ID of a privacy pass token stored locally.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(mirror(TokenId))]
#[frb(dart_metadata = ("freezed"))]
pub struct _TokenId {
    pub id: i64,
    pub created_at: DateTime<Utc>,
}

#[doc(hidden)]
#[frb(mirror(RequestInvitationCodeError))]
#[frb(dart_metadata = ("freezed"))]
pub enum _RequestInvitationCodeError {
    GlobalQuotaExceeded,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, derive_more::From)]
#[frb(dart_metadata = ("freezed"))]
pub enum UiInvitationCode {
    Token(TokenId),
    Code(InvitationCode),
}

#[derive(Debug, Default, Clone)]
#[frb(dart_metadata = ("freezed"))]
pub struct InvitationCodesState {
    pub(crate) codes: Vec<UiInvitationCode>,
}

#[frb(opaque)]
pub struct InvitationCodesCubitBase {
    core: CubitCore<InvitationCodesState>,
    core_user: CoreUser,
}

impl InvitationCodesCubitBase {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let core_user = user_cubit.core_user().clone();

        let core = CubitCore::new();
        let emit_initial_state_task =
            core.cancellation_token()
                .clone()
                .run_until_cancelled_owned(load_and_emit_state(
                    core_user.clone(),
                    core.state_tx().clone(),
                ));

        spawn_from_sync(emit_initial_state_task);

        Self { core, core_user }
    }

    // Cubit interface

    pub fn close(&self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> InvitationCodesState {
        self.core.state()
    }

    pub async fn stream(&self, sink: StreamSink<InvitationCodesState>) {
        self.core.stream(sink).await;
    }

    pub async fn request_invitation_code(
        &self,
        token_id: TokenId,
    ) -> anyhow::Result<Option<RequestInvitationCodeError>> {
        let _ = match self.core_user.request_invitation_code(token_id).await? {
            Ok(code) => code,
            Err(e @ RequestInvitationCodeError::GlobalQuotaExceeded) => return Ok(Some(e)),
        };

        load_and_emit_state(self.core_user.clone(), self.core.state_tx().clone()).await;

        Ok(None)
    }

    pub async fn mark_invitation_code_as_copied(&self, copied_code: &str) -> anyhow::Result<()> {
        self.core_user
            .mark_invitation_code_as_copied(copied_code)
            .await?;

        self.core.state_tx().send_modify(|state| {
            for invitiation_code in &mut state.codes {
                if let UiInvitationCode::Code(code) = invitiation_code
                    && code.code == copied_code
                {
                    code.copied = true;
                    break;
                }
            }
        });

        Ok(())
    }
}

async fn load_and_emit_state(core_user: CoreUser, state_tx: Sender<InvitationCodesState>) {
    if let Err(error) = try_load_and_emit_state(core_user, state_tx).await {
        error!(%error, "failed to load invitation codes from local DB");
    }
}

async fn try_load_and_emit_state(
    core_user: CoreUser,
    state_tx: Sender<InvitationCodesState>,
) -> anyhow::Result<()> {
    let mut codes = core_user.load_invitation_codes().await?;
    codes.sort_unstable_by(|a, b| a.created_at.cmp(&b.created_at).then(a.code.cmp(&b.code)));

    let mut token_ids = core_user.load_invitation_token_ids().await?;
    token_ids.sort_unstable_by_key(|token| (token.created_at, token.id));

    let codes = codes
        .into_iter()
        .map(UiInvitationCode::Code)
        .chain(token_ids.into_iter().map(UiInvitationCode::Token));

    state_tx.send_modify(|state| state.codes = codes.collect());
    Ok(())
}
