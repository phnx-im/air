// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/mention_pill/mention_pill_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/chat_list/chat_list_item_tokens.dart';
import 'package:flutter/widgets.dart';

/// `@` badge beside the unread counter of a chat whose unread messages mention
/// the reader.
///
/// Wears the colors of the reader's own [MentionPillVariant.me] pill. The badge
/// and the pill in the message then read as one signal. The glyph uses the
/// counter's label style and matches its size and weight.
class ChatListMentionMarker extends StatelessWidget {
  const ChatListMentionMarker({super.key, required this.tokens});

  final ChatListItemTokens tokens;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Container(
      width: tokens.mentionMarkerSize,
      height: tokens.mentionMarkerSize,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: MentionPillTokens.fill(palette, MentionPillVariant.me),
        shape: BoxShape.circle,
      ),
      child: Text(
        '@',
        style: typeScale.body.mini.style(
          color: MentionPillTokens.label(palette, MentionPillVariant.me),
          weight: Weight.emphasized,
          tight: true,
        ),
      ),
    );
  }
}
