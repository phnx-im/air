// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#![allow(unused)]

use airprotos::client::link_preview::LinkPreviewCard;
use tokio_util::sync::CancellationToken;

use crate::clients::CoreUser;

impl CoreUser {
    pub async fn fetch_link_preview(
        &self,
        url: &str,
        cancel: CancellationToken,
    ) -> anyhow::Result<Option<FetchedLinkPreview>> {
        self.inner.link_preview_fetcher.fetch(url, cancel).await
    }
}

#[derive(Debug)]
pub struct FetchedLinkPreview {
    card: LinkPreviewCard,
    host: String,
    /// WebP bytes of the card image
    ///
    /// `Some` iff the card has an image.
    image: Option<Vec<u8>>,
}

impl FetchedLinkPreview {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn card(&self) -> &LinkPreviewCard {
        &self.card
    }

    pub fn image_bytes(&self) -> Option<&[u8]> {
        self.image.as_deref()
    }

    pub(crate) fn into_parts(self) -> (LinkPreviewCard, Option<Vec<u8>>) {
        (self.card, self.image)
    }
}

#[derive(Debug)]
pub(crate) struct LinkPreviewFetcher {
    /// Own hardened HTTP client
    client: reqwest::Client,
}

impl LinkPreviewFetcher {
    pub(crate) fn new() -> anyhow::Result<Self> {
        todo!()
    }

    pub(crate) async fn fetch(
        &self,
        url: &str,
        cancel: CancellationToken,
    ) -> anyhow::Result<Option<FetchedLinkPreview>> {
        todo!()
    }
}
