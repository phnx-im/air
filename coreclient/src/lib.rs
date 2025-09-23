// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Implements the protocol logic of the client component

mod chats;
pub mod clients;
mod contacts;
mod groups;
mod key_stores;
pub mod store;
mod user_handles;
mod user_profiles;
mod utils;

pub use crate::{
    chats::{
        Chat, ChatAttributes, ChatId, ChatStatus, ChatType, InactiveChat, MessageDraft,
        messages::{
            ChatMessage, ContentMessage, ErrorMessage, EventMessage, Message, MessageId,
            SystemMessage,
        },
    },
    clients::attachment::{
        AttachmentContent, AttachmentStatus, AttachmentUrl, AttachmentUrlParseError,
        DownloadProgress, DownloadProgressEvent, MimiContentExt,
    },
    clients::block_contact::BlockedContactError,
    contacts::Contact,
    user_handles::UserHandleRecord,
    user_profiles::{Asset, DisplayName, DisplayNameError, UserProfile},
    utils::persistence::{delete_client_database, delete_databases, open_client_db},
};
