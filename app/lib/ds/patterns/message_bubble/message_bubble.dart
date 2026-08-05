// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble_tokens.dart';
import 'package:flutter/widgets.dart';

/// How a bubble carries its content.
enum MessageBubbleVariant {
  /// A message body: a filled, rounded tile.
  filled,

  /// A stand-in for a body that's gone -- a deleted message. Outlined in the
  /// fill the bubble would have had, so the message keeps its place in the
  /// column without claiming the weight of one that still has content.
  outlined,

  /// Content that's its own shape -- an emoji-only body. No fill, no outline,
  /// the glyph stands on the conversation.
  naked,
}

/// The shell of a single message: the shape, the fill, and the content inset.
///
/// Hugs what it carries, so everything that has to line up with the bubble --
/// the reaction strip, the hover buttons, the jump highlight -- gets the
/// bubble's own box. How wide it may get and which side it sits on belong to
/// the row around it.
///
/// A pure view over one [child]. Everything inside -- the body, a quoted reply,
/// an attachment, the metadata stamp -- is the host's, so this pattern stays
/// the one place the bubble's geometry is decided.
class MessageBubble extends StatelessWidget {
  const MessageBubble({
    super.key,
    required this.tokens,
    required this.isSelf,
    required this.child,
    this.variant = MessageBubbleVariant.filled,
    this.padding,
    this.intrinsicWidth = false,
  });

  final MessageBubbleTokens tokens;

  /// Own message. Picks the fill.
  final bool isSelf;

  final Widget child;

  final MessageBubbleVariant variant;

  /// Content inset override. Defaults to [MessageBubbleTokens.padding]. Pass
  /// [EdgeInsets.zero] for stacked content whose blocks carry their own inset,
  /// so an image can run to the bubble's edge while the text beside it
  /// doesn't.
  final EdgeInsets? padding;

  /// Sizes the bubble to its widest block instead of to the text it wraps.
  /// Stacked content -- a quoted reply above a one-word answer -- needs it so
  /// the blocks share a width rather than each hugging its own.
  final bool intrinsicWidth;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final fill = isSelf
        ? palette.message.selfBackground
        : palette.message.otherBackground;
    final corners = BorderRadius.circular(tokens.radius);

    final decoration = switch (variant) {
      MessageBubbleVariant.filled => BoxDecoration(
        color: fill,
        borderRadius: corners,
      ),
      MessageBubbleVariant.outlined => BoxDecoration(
        border: Border.all(color: fill, width: tokens.borderWidth),
        borderRadius: corners,
      ),
      MessageBubbleVariant.naked => null,
    };

    final bubble = Container(
      padding: padding ?? tokens.padding,
      decoration: decoration,
      // Clipped to the bubble's own corners, so full-bleed content needs no
      // radius of its own.
      clipBehavior: decoration == null ? Clip.none : Clip.antiAlias,
      child: child,
    );

    return intrinsicWidth ? IntrinsicWidth(child: bubble) : bubble;
  }
}
