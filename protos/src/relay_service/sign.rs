use aircommon::crypto::signatures::keys::QsUserVerifyingKeyType;

use crate::relay_service::v1::{LinkClientRequest, LinkClientRequestPayload};

impl_signed_payload!(
    LinkClientRequest,
    LinkClientRequestPayload,
    QsUserVerifyingKeyType,
    "LinkClientRequestPayload"
);
