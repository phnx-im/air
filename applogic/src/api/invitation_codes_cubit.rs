// Re-export for FRB reasons
pub(crate) use aircoreclient::{InvitationCode, RequestInvitationCodeError};

use aircoreclient::clients::CoreUser;
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
}

#[doc(hidden)]
#[frb(mirror(RequestInvitationCodeError))]
#[frb(dart_metadata = ("freezed"))]
pub enum _RequestInvitationCodeError {
    UserQuotaExceeded,
    GlobalQuotaExceeded,
}

#[derive(Debug, Default, Clone)]
#[frb(dart_metadata = ("freezed"))]
pub struct InvitationCodesState {
    pub(crate) codes: Vec<InvitationCode>,
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
    ) -> anyhow::Result<Option<RequestInvitationCodeError>> {
        let code = match self.core_user.request_invitation_code().await {
            Ok(code) => code,
            Err(e @ RequestInvitationCodeError::UserQuotaExceeded) => return Ok(Some(e)),
            Err(e @ RequestInvitationCodeError::GlobalQuotaExceeded) => return Ok(Some(e)),
            Err(e) => return Err(e.into()),
        };
        self.core.state_tx().send_modify(|state| {
            state.codes.push(code);
        });

        Ok(None)
    }

    pub async fn mark_invitation_code_as_copied(&self, code: &str) -> anyhow::Result<()> {
        self.core_user.mark_invitation_code_as_copied(code).await?;

        self.core.state_tx().send_modify(|state| {
            for saved_code in &mut state.codes {
                if saved_code.code == code {
                    saved_code.copied = true;
                }
            }
        });

        Ok(())
    }
}

async fn load_and_emit_state(core_user: CoreUser, state_tx: Sender<InvitationCodesState>) {
    match core_user.load_invitation_codes().await {
        Ok(codes) => {
            state_tx.send_modify(|state| {
                state.codes = codes;
            });
        }
        Err(error) => {
            error!(%error, "failed to load invitation codes from local DB");
        }
    }
}
