// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::identifiers::UserId;
use mimi_content::MimiContent;
use pulldown_cmark::{Event, Options, Parser, Tag};

use crate::MimiContentExt;

/// Markdown options of the message renderer in applogic and of mention
/// extraction. Sharing them keeps both in agreement on the links of a message.
///
/// Do not enable `Options::ENABLE_GFM`, it activates special blockquotes which
/// are not part of the GFM spec
/// <https://github.com/orgs/community/discussions/16925>.
pub const MARKDOWN_OPTIONS: Options = Options::ENABLE_STRIKETHROUGH
    .union(Options::ENABLE_TABLES)
    .union(Options::ENABLE_TASKLISTS);

/// Extracts all user mentions from a markdown string.
///
/// A mention is a markdown link whose destination is a mimi user URI
/// (`mimi://<domain>/u/<uuid>`). Code blocks and escaped links never
/// produce link events, so they are not scanned.
fn extract_mentions(markdown: &str) -> Vec<UserId> {
    Parser::new_ext(markdown, MARKDOWN_OPTIONS)
        .filter_map(|event| match event {
            Event::Start(Tag::Link { dest_url, .. }) => UserId::parse_uri(&dest_url),
            _ => None,
        })
        .collect()
}

/// Returns the users that the markdown body of `content` mentions, not yet
/// checked against the chat members.
pub(crate) fn mentioned_users(content: &MimiContent) -> Vec<UserId> {
    content
        .markdown_text()
        .map(extract_mentions)
        .unwrap_or_default()
}

/// Keeps the first mention of each member and drops all other mentions.
///
/// The member set is only built when there is a mention, since most messages
/// have none.
pub(crate) fn retain_members(
    mentions: &mut Vec<UserId>,
    members: impl IntoIterator<Item = UserId>,
) {
    if mentions.is_empty() {
        return;
    }
    let members: HashSet<UserId> = members.into_iter().collect();
    mentions.retain(|user| members.contains(user));
    dedup_in_order(mentions);
}

/// Keeps the first mention of each user and drops all other mentions.
pub(crate) fn dedup_in_order(mentions: &mut Vec<UserId>) {
    let mut seen = HashSet::new();
    mentions.retain(|user| seen.insert(user.clone()));
}

/// Returns the mentions of `members` in `content`, deduplicated in order of
/// first appearance.
///
/// Only links to current chat members count as mentions. Callers check this
/// at receive, send or edit time.
pub(crate) fn valid_mentions(
    content: &MimiContent,
    members: impl IntoIterator<Item = UserId>,
) -> Vec<UserId> {
    let mut mentions = mentioned_users(content);
    retain_members(&mut mentions, members);
    mentions
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::Fqdn;
    use uuid::uuid;

    use super::*;

    fn alice() -> UserId {
        UserId::new(
            uuid!("00000000-0000-0000-0000-000000000001"),
            "example.com".parse::<Fqdn>().unwrap(),
        )
    }

    fn bob() -> UserId {
        UserId::new(
            uuid!("00000000-0000-0000-0000-000000000002"),
            "example.com".parse::<Fqdn>().unwrap(),
        )
    }

    #[test]
    fn extracts_mentions() {
        let alice = alice();
        let bob = bob();
        let markdown = format!(
            "Kudos to [@Alice]({}) and [@Bob]({}) for making it happen!",
            alice.uri(),
            bob.uri()
        );
        assert_eq!(extract_mentions(&markdown), vec![alice, bob]);
    }

    #[test]
    fn extracts_mentions_in_nested_formatting() {
        let alice = alice();
        let markdown = format!("> a quote with **[@Alice]({})** in a list\n", alice.uri());
        assert_eq!(extract_mentions(&markdown), vec![alice]);
    }

    #[test]
    fn ignores_non_mention_links() {
        let markdown = "a [link](https://example.com) and \
            a [non-user](mimi://example.com/attachment/x) and \
            a [non-uuid](mimi://example.com/u/not-a-uuid)";
        assert_eq!(extract_mentions(markdown), vec![]);
    }

    #[test]
    fn ignores_code_and_escaped_links() {
        let alice = alice();
        let uri = alice.uri();
        let markdown =
            format!("`[@Alice]({uri})`\n\n```\n[@Alice]({uri})\n```\n\n\\[@Alice\\]({uri})");
        assert_eq!(extract_mentions(&markdown), vec![]);
    }

    #[test]
    fn ignores_images() {
        let markdown = format!("![@Alice]({})", alice().uri());
        assert_eq!(extract_mentions(&markdown), vec![]);
    }

    #[test]
    fn valid_mentions_filters_non_members() {
        let alice = alice();
        let bob = bob();
        let content = MimiContent::simple_markdown_message(
            format!("hi [@Alice]({}) and [@Bob]({})", alice.uri(), bob.uri()),
            [0; 16],
        );
        assert_eq!(
            valid_mentions(&content, [alice.clone(), bob.clone()]),
            vec![alice.clone(), bob.clone()]
        );
        assert_eq!(valid_mentions(&content, [alice.clone()]), vec![alice]);
        assert_eq!(valid_mentions(&content, []), vec![]);
    }

    #[test]
    fn valid_mentions_deduplicates() {
        let alice = alice();
        let content = MimiContent::simple_markdown_message(
            format!(
                "[@Alice]({uri}) and again [@Alice]({uri})",
                uri = alice.uri()
            ),
            [0; 16],
        );
        assert_eq!(valid_mentions(&content, [alice.clone()]), vec![alice]);
    }

    #[test]
    fn valid_mentions_empty_without_mentions() {
        let content = MimiContent::simple_markdown_message("no mention here".to_owned(), [0; 16]);
        assert_eq!(valid_mentions(&content, [alice()]), vec![]);
    }
}
