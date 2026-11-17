// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{sync::Arc, time::Duration};

use airprotos::client::link_preview::{LinkPreviewCard, LinkPreviewImage, LinkPreviewKind};
use percent_encoding::percent_decode_str;
use reqwest::{header, redirect};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use url::Url;

use crate::{
    MediaType,
    clients::CoreUser,
    link_preview::{fetch::page::Page, send::LINK_PREVIEW_IMAGE_PART_INDEX},
};

use super::links::is_selectable;

use image::{IMAGE_TYPES, PreviewImage};
use resolver::PublicResolver;

mod image;
mod page;
mod resolver;

impl CoreUser {
    pub async fn fetch_link_preview(
        &self,
        url: &str,
        cancel: CancellationToken,
    ) -> Option<FetchedLinkPreview> {
        self.inner.link_preview_fetcher.fetch(url, cancel).await
    }
}

#[derive(Debug, Clone)]
pub struct FetchedLinkPreview {
    card: LinkPreviewCard,
    /// WebP bytes of the card image
    ///
    /// `Some` iff the card has an image.
    image: Option<Vec<u8>>,
}

impl FetchedLinkPreview {
    /// Preview of `card` with the WebP bytes of its image.
    ///
    /// Returns `None` if only one of the card and `image` has an image.
    pub fn new(card: LinkPreviewCard, image: Option<Vec<u8>>) -> Option<Self> {
        if card.image().is_some() != image.is_some() {
            return None;
        }
        Some(Self { card, image })
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
    /// Dedicated hardened HTTP client
    client: reqwest::Client,
}

impl LinkPreviewFetcher {
    pub(crate) fn new() -> anyhow::Result<Self> {
        const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
        const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
        const USER_AGENT: &str = "WhatsApp/2";

        let client = reqwest::Client::builder()
            // Allow only https requests, including redirects.
            .https_only(true)
            // Disable proxies, also from environment. Proxies resolve DNS which bypasses DNS
            // filtering.
            .no_proxy()
            // No Referer on redirects, see
            // <https://developer.mozilla.org/en-US/docs/Web/Privacy/Guides/Referer_header:_privacy_and_security_concerns>.
            .referer(false)
            .redirect(redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            // Reject non-public IP addresses
            .dns_resolver(Arc::new(PublicResolver))
            .build()?;
        Ok(Self { client })
    }

    pub(crate) async fn fetch(
        &self,
        url: &str,
        cancel: CancellationToken,
    ) -> Option<FetchedLinkPreview> {
        const TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
        let fetch = timeout(TOTAL_TIMEOUT, self.do_fetch(url));
        match cancel.run_until_cancelled(fetch).await? {
            Ok(Ok(preview)) => preview,
            Ok(Err(error)) => {
                debug!(%error, "failed to fetch link preview");
                None
            }
            Err(_elapsed) => {
                debug!("link preview fetch timed out");
                None
            }
        }
    }

    async fn do_fetch(&self, url: &str) -> anyhow::Result<Option<FetchedLinkPreview>> {
        let link = Url::parse(url)?;

        let image_types = IMAGE_TYPES.map(|(ty, _)| ty).join(",");
        let accept = format!("text/html,application/xhtml+xml,{image_types}");
        let Some(response) = self.get(link.clone(), &accept).await? else {
            return Ok(None);
        };

        let (title, description, kind, image) = match ContentKind::of(&response) {
            Some(ContentKind::Html) => {
                // og:image is relative to where redirects ended
                let final_url = response.url().clone();
                let Some(page) = Page::read(response).await? else {
                    return Ok(None);
                };
                let image = match page.image.and_then(|image| final_url.join(&image).ok()) {
                    // A broken image still leaves the card
                    Some(image_url) => self.fetch_image(image_url).await.ok().flatten(),
                    None => None,
                };
                (page.title, page.description, page.kind, image)
            }
            Some(ContentKind::Image) => {
                let Some(title) = file_title(&link) else {
                    return Ok(None);
                };
                let Some(image) = PreviewImage::read(response).await? else {
                    return Ok(None);
                };
                (title, String::new(), LinkPreviewKind::Default, Some(image))
            }
            None => return Ok(None),
        };

        let card_image = image.as_ref().and_then(|image| {
            LinkPreviewImage::new(
                LINK_PREVIEW_IMAGE_PART_INDEX.0,
                image.width,
                image.height,
                Some(image.blurhash.clone()),
                None,
            )
        });
        let image = image
            .filter(|_| card_image.is_some())
            .map(|image| image.webp);
        // The card keeps the link as written, not the parsed form
        let Ok(card) = LinkPreviewCard::new(url.to_owned(), &title, &description, kind, card_image)
        else {
            return Ok(None);
        };
        Ok(FetchedLinkPreview::new(card, image))
    }

    /// GET `url`, following redirects that pass the link rules from [`super::links`].
    async fn get(&self, mut url: Url, accept: &str) -> anyhow::Result<Option<reqwest::Response>> {
        const MAX_REDIRECTS: usize = 10;
        for _ in 0..MAX_REDIRECTS + 1 {
            if !is_fetchable(&url) {
                return Ok(None);
            }
            let response = self
                .client
                .get(url.clone())
                .header(header::ACCEPT, accept)
                .send()
                .await?;
            if !response.status().is_redirection() {
                return Ok(response.status().is_success().then_some(response));
            }
            // The body of a redirect is never read
            let Some(next) = response
                .headers()
                .get(header::LOCATION)
                .and_then(|location| location.to_str().ok())
                .and_then(|location| url.join(location).ok())
            else {
                return Ok(None);
            };
            url = next;
        }
        Ok(None)
    }
}

fn is_fetchable(url: &Url) -> bool {
    url.port().is_none() && is_selectable(url.as_str())
}

enum ContentKind {
    Html,
    Image,
}

impl ContentKind {
    fn of(response: &reqwest::Response) -> Option<Self> {
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)?
            .to_str()
            .ok()?;
        let media_type = MediaType::of(content_type);
        if media_type == "text/html" || media_type == "application/xhtml+xml" {
            Some(Self::Html)
        } else if IMAGE_TYPES.iter().any(|(ty, _)| media_type == *ty) {
            Some(Self::Image)
        } else {
            None
        }
    }
}

fn file_title(url: &Url) -> Option<String> {
    let segment = url.path_segments()?.next_back()?;
    let title = percent_decode_str(segment).decode_utf8_lossy().into_owned();
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_title_is_the_last_segment() {
        for (url, title) in [
            ("https://a.org/dir/photo.jpg", Some("photo.jpg")),
            ("https://a.org/My%20Photo.png", Some("My Photo.png")),
            ("https://a.org/a.png?x=1#y", Some("a.png")),
            ("https://a.org/%FF.png", Some("\u{FFFD}.png")),
            ("https://a.org/dir/", None),
            ("https://a.org", None),
        ] {
            let url = Url::parse(url).unwrap();
            assert_eq!(file_title(&url).as_deref(), title, "{url}");
        }
    }
}
