// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for a message bubble.
///
/// Geometry only: colors come from the palette at paint time. One set for
/// every density -- a bubble's content and the width of the conversation size
/// it, never the pointer.
@immutable
class MessageBubbleTokens {
  const MessageBubbleTokens({
    required this.padding,
    required this.radius,
    required this.borderWidth,
  });

  /// Inset between the bubble's edge and its content. A bubble whose content
  /// runs to the edge (an image, a gallery) takes [EdgeInsets.zero], and its
  /// blocks carry this inset themselves.
  final EdgeInsets padding;

  /// Corner radius, the same on all four corners. A flight of bubbles keeps one
  /// shape: the gap between them already reads as a run, so stepping the
  /// corners down only makes the column look ragged.
  final double radius;

  /// Outline of the [MessageBubbleVariant.outlined] look.
  final double borderWidth;

  static const MessageBubbleTokens standard = MessageBubbleTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    radius: CornerRadius.px12,
    borderWidth: StrokeWidth.px1,
  );
}
