// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod card;
mod draft;
mod fetch;
mod layout;
mod links;

pub use card::LinkPreview;
pub use draft::DraftLinkPreview;
pub use fetch::FetchedLinkPreview;
pub use links::select_links;

pub(crate) use fetch::LinkPreviewFetcher;
pub(crate) use layout::{CardImage, editable_text, with_fetched_card, with_original_card};
