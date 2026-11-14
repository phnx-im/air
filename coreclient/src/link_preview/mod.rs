// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod card;
mod fetch;
mod links;
mod send;

pub use card::LinkPreview;
pub use fetch::FetchedLinkPreview;
pub use links::select_links;

pub(crate) use fetch::LinkPreviewFetcher;
pub(crate) use send::{carry_over_link_preview, editable_text};
