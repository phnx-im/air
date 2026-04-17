// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Implements the protocol logic of the client component

#![warn(clippy::large_futures)]

mod chats;
pub mod clients;
mod contacts;
mod groups;
mod job;
mod key_stores;
pub mod outbound_service;
pub(crate) mod privacy_pass;
pub mod store;
mod user_handles;
mod user_profiles;
mod utils;

pub use crate::{
    chats::{
        Chat, ChatAttributes, ChatId, ChatStatus, ChatType, InactiveChat, MessageDraft,
        messages::{
            ChatMessage, ContentMessage, ErrorMessage, EventMessage, InReplyToMessage, Message,
            MessageId, SystemMessage,
        },
        pending::AcceptContactRequestError,
    },
    clients::{
        add_contact::AddHandleContactError,
        attachment::{
            AttachmentContent, AttachmentStatus, AttachmentUrl, AttachmentUrlParseError,
            MimiContentExt, ProvisionAttachmentError, UploadTaskError,
            progress::{AttachmentProgress, AttachmentProgressEvent},
        },
        block_contact::BlockedContactError,
        invitation_code::{InvitationCode, RequestInvitationCodeError},
        invite_users::InviteUsersError,
        safety_code::SafetyCode,
        user_settings::ReadReceiptsSetting,
    },
    contacts::{Contact, ContactType, PartialContact, TargetedMessageContact},
    groups::debug_info::{
        AirComponentDebugInfo, AppDataDebugInfo, DebugCapabilities, EncryptedGroupTitleDebugInfo,
        ExternalGroupProfileDebugInfo, GroupDataDebugInfo, GroupDebugInfo,
        RequiredDebugCapabilities,
    },
    privacy_pass::{RequestTokensError, TokenId},
    user_handles::UserHandleRecord,
    user_profiles::{Asset, DisplayName, DisplayNameError, UserProfile},
    utils::persistence::{
        delete_client_database, delete_databases, export_client_database, import_client_database,
        open_client_db,
    },
};
