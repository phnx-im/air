// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Implements the protocol logic of the client component

#![warn(clippy::large_futures)]

mod chats;
pub mod clients;
mod contacts;
mod groups;
mod key_stores;
pub mod outbound_service;
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
    clients::{
        add_contact::{AddHandleContactError, AddHandleContactResult},
        attachment::{
            AttachmentContent, AttachmentStatus, AttachmentUrl, AttachmentUrlParseError,
            MimiContentExt,
            progress::{AttachmentProgress, AttachmentProgressEvent},
        },
        block_contact::BlockedContactError,
    },
    contacts::{Contact, ContactType, PartialContact, TargetedMessageContact},
    user_handles::UserHandleRecord,
    user_profiles::{Asset, DisplayName, DisplayNameError, UserProfile},
    utils::persistence::{
        delete_client_database, delete_databases, export_client_database, import_client_database,
        open_client_db,
    },
};
