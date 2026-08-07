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
abstract final class MessageBubbleTokens {
  /// Inset between the bubble's edge and its content. A bubble whose content
  /// runs to the edge (an image, a gallery) takes [EdgeInsets.zero], and its
  /// blocks carry this inset themselves.
  static const EdgeInsets padding = EdgeInsets.symmetric(
    horizontal: S.s12,
    vertical: S.s8,
  );

  /// Corner radius, the same on all four corners. A group of bubbles keeps one
  /// shape: the gap between them already reads as a run, so stepping the
  /// corners down only makes the column look ragged.
  static const double radius = CornerRadius.px12;

  /// Outline of the [MessageBubbleVariant.outlined] look.
  static const double borderWidth = StrokeWidth.px1;
}
