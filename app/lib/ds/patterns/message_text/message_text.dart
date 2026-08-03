// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_text/message_text_tokens.dart';
import 'package:flutter/widgets.dart';

/// A message body: the blocks of one message, stacked and styled.
///
/// The host renders the blocks -- a body is markdown, and parsing it is the
/// host's job -- so this pattern owns only what makes them read as one
/// message: the body type, the ink for the side of the conversation the
/// message came from, and the space between blocks. [styleOf] hands that same
/// style to a renderer that styles its own spans.
class MessageText extends StatelessWidget {
  const MessageText({
    super.key,
    required this.tokens,
    required this.isSelf,
    required this.blocks,
    this.jumbo = false,
  });

  final MessageTextTokens tokens;

  /// Own message. Picks the ink that reads against the bubble's fill.
  final bool isSelf;

  /// Rendered blocks of the body, top to bottom: paragraphs, lists, quotes,
  /// code, tables. Each inherits the resolved body style, so a block only
  /// styles what makes it different.
  final List<Widget> blocks;

  /// An emoji-only body, shown at glyph size instead of body size -- a bare
  /// reaction to the message above rather than a sentence.
  final bool jumbo;

  /// The body style for a message from this side of the conversation. Exposed
  /// so a rich-text renderer builds its spans from the same source the plain
  /// blocks inherit.
  static TextStyle styleOf(
    BuildContext context, {
    required bool isSelf,
    bool jumbo = false,
  }) {
    final message = SemanticPalette.of(context).message;
    final color = isSelf ? message.selfText : message.otherText;
    return jumbo
        ? typeScale.emoji.jumbo.style(color: color)
        : typeScale.body.regular.style(color: color);
  }

  /// The style for a body that stands in for one that's not there -- deleted,
  /// or withheld until the reader asks for it. Italic and quiet, so it never
  /// passes for something the sender wrote.
  static TextStyle placeholderStyleOf(BuildContext context) => typeScale
      .body
      .regular
      .style(color: SemanticPalette.of(context).text.tertiary)
      .copyWith(fontStyle: FontStyle.italic);

  @override
  Widget build(BuildContext context) {
    return DefaultTextStyle(
      style: styleOf(context, isSelf: isSelf, jumbo: jumbo),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        spacing: tokens.blockGap,
        children: blocks,
      ),
    );
  }
}
