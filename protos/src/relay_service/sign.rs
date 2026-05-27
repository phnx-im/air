use aircommon::crypto::signatures::{keys::QsUserVerifyingKeyType, signable::Verifiable};

use crate::{
    common::v1::Signature,
    relay_service::v1::{LinkClientRequest, LinkClientRequestPayload},
};

impl_signed_payload!(
    LinkClientRequest,
    LinkClientRequestPayload,
    QsUserVerifyingKeyType,
    "LinkClientRequestPayload"
);
