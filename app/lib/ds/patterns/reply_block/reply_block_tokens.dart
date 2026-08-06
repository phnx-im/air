// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a quoted message.
///
/// Geometry only: colors come from the palette at paint time, and the
/// surface the block sits on is the host's to supply. One set for both
/// densities, the block is two short lines of text either way.
abstract final class ReplyBlockTokens {
  /// Inset between the fill and the quoted text. Applies only where the block
  /// carries a fill: unfilled, the accent rule is the block's leading edge.
  static const EdgeInsets padding = EdgeInsets.symmetric(
    horizontal: S.s12,
    vertical: S.s8,
  );

  static const double radius = CornerRadius.px8;

  /// The rule down the leading edge, which is what marks the text as quoted.
  static const double accentWidth = StrokeWidth.px1;

  /// Gap between the accent rule and the quoted text.
  static const double accentGap = S.s12;

  /// Gap between the sender's name and the preview under it.
  static const double senderGap = S.s2;

  static const double iconSize = S.s12;

  /// Gap between the quoted text and the jump indicator.
  static const double iconGap = S.s8;

  /// Drops the jump indicator onto the optical centre of the sender line,
  /// which the glyph's own box doesn't land on.
  static const double iconTopOffset = 6;

  /// The still of a quoted picture, which trails the text where the jump
  /// indicator otherwise would.
  static const double thumbSize = S.s32;
  static const double thumbRadius = CornerRadius.px4;

  /// Lines of the quoted message shown before it ellipsizes. Two: enough to
  /// recognize the message, few enough that the quote stays smaller than the
  /// reply to it.
  static const int previewMaxLines = 2;
}
