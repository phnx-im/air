// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Implements the protocol logic of the client component

#![warn(clippy::large_futures)]

mod chats;
pub mod clients;
mod contacts;
pub(crate) mod db_access;
mod groups;
mod job;
mod key_stores;
pub mod outbound_service;
pub(crate) mod privacy_pass;
pub mod store;
mod user_profiles;
mod usernames;
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
        add_contact::AddUsernameContactError,
        attachment::{
            AttachmentContent, AttachmentStatus, AttachmentUrl, AttachmentUrlParseError,
            MimiContentExt, ProvisionAttachmentError, UploadTaskError,
            progress::{AttachmentProgress, AttachmentProgressEvent},
        },
        block_contact::BlockedContactError,
        debug_info::{TimedTaskDebugInfo, UserDebugInfo},
        invitation_code::{InvitationCode, RequestInvitationCodeError},
        invite_users::InviteUsersError,
        safety_code::SafetyCode,
        user_settings::ReadReceiptsSetting,
    },
    contacts::{Contact, ContactType, PartialContact, TargetedMessageContact},
    groups::debug_info::{
        AppDataDebugInfo, DebugCapabilities, EncryptedGroupTitleDebugInfo,
        ExternalGroupProfileDebugInfo, GroupDataDebugInfo, GroupDebugInfo,
        RequiredDebugCapabilities,
    },
    privacy_pass::{RequestTokensError, TokenId},
    user_profiles::{Asset, DisplayName, DisplayNameError, UserProfile},
    usernames::UsernameRecord,
    utils::{
        image::image_is_animated,
        persistence::{delete_client_database, delete_databases, open_client_db},
    },
};
