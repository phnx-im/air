// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, mem};

use aircoreclient::link_preview::{FetchedLinkPreview, select_links};
use tokio_util::sync::CancellationToken;

use crate::api::message_content::UiComposerLinkPreview;

/// Link preview of the message being composed.
///
/// Follows the first link of the composer text, with at most one fetch
/// running at a time.
#[derive(Debug, Default)]
pub(crate) struct ComposerLinkPreview {
    url: Option<String>,
    fetch: Fetch,
    dismissed: HashSet<String>,
}

#[derive(Debug, Default)]
enum Fetch {
    #[default]
    None,
    Loading(CancellationToken),
    Ready(FetchedLinkPreview),
}

impl ComposerLinkPreview {
    /// Copy of the ready preview if it belongs to the first link of `text`.
    pub(crate) fn ready_for(&self, text: &str) -> Option<FetchedLinkPreview> {
        let first = select_links(text).next()?;
        match &self.fetch {
            Fetch::Ready(fetched) if fetched.card().url() == first => Some(fetched.clone()),
            _ => None,
        }
    }

    /// Image bytes of the ready preview of `url`.
    pub(crate) fn image_for(&self, url: &str) -> Option<Vec<u8>> {
        match &self.fetch {
            Fetch::Ready(fetched) if fetched.card().url() == url => {
                fetched.image_bytes().map(|bytes| bytes.to_vec())
            }
            _ => None,
        }
    }

    /// Resets the state after a sent message and cancels a running fetch.
    pub(crate) fn reset(&mut self) {
        self.cancel();
        self.url = None;
        self.dismissed.clear();
    }

    /// Makes `url` the current link.
    ///
    /// Returns the url to fetch and the token of the fetch, which is a child
    /// of `parent`. Returns `None` if there is nothing to fetch, because the
    /// link did not change, was dismissed or there is none.
    pub(crate) fn switch_to<'a>(
        &mut self,
        url: Option<&'a str>,
        parent: &CancellationToken,
    ) -> Option<(&'a str, CancellationToken)> {
        if self.url.as_deref() == url {
            return None;
        };
        self.cancel();
        self.url = url.map(|s| s.to_owned());
        let url = url.filter(|url| !self.dismissed.contains(*url))?;
        let token = parent.child_token();
        self.fetch = Fetch::Loading(token.clone());
        Some((url, token))
    }

    /// Drops the preview of `url` and does not fetch it again.
    pub(crate) fn dismiss(&mut self, url: &str) {
        if self.url.as_deref() == Some(url) {
            self.cancel();
        }
        self.dismissed.insert(url.to_owned());
    }

    /// Stores the result of the fetch started with `token`.
    ///
    /// The result of a cancelled fetch is ignored.
    pub(crate) fn finish(
        &mut self,
        token: &CancellationToken,
        fetched: Option<FetchedLinkPreview>,
    ) {
        if !token.is_cancelled() {
            self.fetch = fetched.map(Fetch::Ready).unwrap_or(Fetch::None);
        }
    }

    pub(crate) fn ui(&self) -> Option<UiComposerLinkPreview> {
        match (&self.url, &self.fetch) {
            (Some(url), Fetch::Loading(_)) => {
                Some(UiComposerLinkPreview::Loading { url: url.clone() })
            }
            (_, Fetch::Ready(fetched)) => Some(UiComposerLinkPreview::ready(fetched)),
            _ => None,
        }
    }

    /// Drops the preview and cancels a running fetch.
    fn cancel(&mut self) {
        if let Fetch::Loading(token) = mem::take(&mut self.fetch) {
            token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use airprotos::client::link_preview::{LinkPreviewCard, LinkPreviewKind};

    use super::*;

    const A: &str = "https://a.org";
    const B: &str = "https://b.org";

    fn fetched(url: &str) -> FetchedLinkPreview {
        let card =
            LinkPreviewCard::new(url.to_owned(), "title", "", LinkPreviewKind::Default, None)
                .unwrap();
        FetchedLinkPreview::new(card, None).unwrap()
    }

    fn loading(url: &str) -> Option<UiComposerLinkPreview> {
        Some(UiComposerLinkPreview::Loading {
            url: url.to_owned(),
        })
    }

    fn ready(url: &str) -> Option<UiComposerLinkPreview> {
        Some(UiComposerLinkPreview::ready(&fetched(url)))
    }

    /// Preview with a finished fetch of `url`.
    fn with_ready(url: &str) -> ComposerLinkPreview {
        let mut preview = ComposerLinkPreview::default();
        let (_, token) = preview
            .switch_to(Some(url), &CancellationToken::new())
            .unwrap();
        preview.finish(&token, Some(fetched(url)));
        preview
    }

    #[test]
    fn switch_to_starts_one_fetch_per_link() {
        let parent = CancellationToken::new();
        let mut preview = ComposerLinkPreview::default();
        assert_eq!(preview.ui(), None);

        let (url, token) = preview.switch_to(Some(A), &parent).unwrap();
        assert_eq!(url, A);
        assert_eq!(preview.ui(), loading(A));

        assert!(preview.switch_to(Some(A), &parent).is_none());
        assert!(!token.is_cancelled());
        assert_eq!(preview.ui(), loading(A));
    }

    #[test]
    fn switch_to_another_link_cancels_the_fetch() {
        let parent = CancellationToken::new();
        let mut preview = ComposerLinkPreview::default();
        let (_, token_a) = preview.switch_to(Some(A), &parent).unwrap();

        let (url, token_b) = preview.switch_to(Some(B), &parent).unwrap();
        assert_eq!(url, B);
        assert!(token_a.is_cancelled());
        assert!(!token_b.is_cancelled());
        assert_eq!(preview.ui(), loading(B));

        // The late result of the first fetch is ignored
        preview.finish(&token_a, Some(fetched(A)));
        assert_eq!(preview.ui(), loading(B));
    }

    #[test]
    fn switch_to_no_link_drops_the_preview() {
        let parent = CancellationToken::new();
        let mut preview = ComposerLinkPreview::default();
        let (_, token) = preview.switch_to(Some(A), &parent).unwrap();
        assert!(preview.switch_to(None, &parent).is_none());
        assert!(token.is_cancelled());
        assert_eq!(preview.ui(), None);

        let mut preview = with_ready(A);
        assert!(preview.switch_to(None, &parent).is_none());
        assert_eq!(preview.ui(), None);
    }

    #[test]
    fn fetch_is_cancelled_with_its_parent() {
        let parent = CancellationToken::new();
        let mut preview = ComposerLinkPreview::default();
        let (_, token) = preview.switch_to(Some(A), &parent).unwrap();
        parent.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn finish_stores_the_result() {
        assert_eq!(with_ready(A).ui(), ready(A));

        let mut preview = ComposerLinkPreview::default();
        let (_, token) = preview
            .switch_to(Some(A), &CancellationToken::new())
            .unwrap();
        preview.finish(&token, None);
        assert_eq!(preview.ui(), None);
        // A failed fetch is not repeated for the same link
        assert!(
            preview
                .switch_to(Some(A), &CancellationToken::new())
                .is_none()
        );
    }

    #[test]
    fn dismiss_drops_the_preview_and_stops_fetching() {
        let parent = CancellationToken::new();

        let mut preview = ComposerLinkPreview::default();
        let (_, token) = preview.switch_to(Some(A), &parent).unwrap();
        preview.dismiss(A);
        assert!(token.is_cancelled());
        assert_eq!(preview.ui(), None);

        let mut preview = with_ready(A);
        preview.dismiss(A);
        assert_eq!(preview.ui(), None);

        // Not fetched again, also after another link
        assert!(preview.switch_to(Some(B), &parent).is_some());
        assert!(preview.switch_to(Some(A), &parent).is_none());
        assert_eq!(preview.ui(), None);
    }

    #[test]
    fn dismiss_of_another_link_keeps_the_preview() {
        let mut preview = with_ready(A);
        preview.dismiss(B);
        assert_eq!(preview.ui(), ready(A));
    }

    #[test]
    fn ready_for_first_link_of_text() {
        let preview = with_ready(A);
        let text = format!("see {A} and {B}");
        assert_eq!(preview.ready_for(&text).unwrap().card().url(), A);
        // The state is left as is
        assert_eq!(preview.ui(), ready(A));

        assert!(preview.ready_for(&format!("see {B} and {A}")).is_none());
        assert!(preview.ready_for("no link").is_none());
    }

    #[test]
    fn ready_for_needs_a_finished_fetch() {
        let mut preview = ComposerLinkPreview::default();
        assert!(preview.ready_for(A).is_none());
        let _fetch = preview.switch_to(Some(A), &CancellationToken::new());
        assert!(preview.ready_for(A).is_none());
    }

    #[test]
    fn reset_cancels_the_fetch_and_forgets_dismissed_links() {
        let parent = CancellationToken::new();
        let mut preview = ComposerLinkPreview::default();
        preview.dismiss(B);
        let (_, token) = preview.switch_to(Some(A), &parent).unwrap();

        preview.reset();
        assert!(token.is_cancelled());
        assert_eq!(preview.ui(), None);
        preview.finish(&token, Some(fetched(A)));
        assert_eq!(preview.ui(), None);

        // Both links are fetched again
        assert!(preview.switch_to(Some(A), &parent).is_some());
        assert!(preview.switch_to(Some(B), &parent).is_some());
    }

    #[test]
    fn reset_drops_a_ready_preview() {
        let mut preview = with_ready(A);
        preview.reset();
        assert_eq!(preview.ui(), None);
        assert!(preview.ready_for(A).is_none());
    }

    #[test]
    fn image_for_ready_preview_of_url() {
        let preview = with_ready(A);
        // The preview has no image
        assert_eq!(preview.image_for(A), None);
        assert_eq!(preview.image_for(B), None);
    }
}
