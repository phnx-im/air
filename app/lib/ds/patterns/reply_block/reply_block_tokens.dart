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
@immutable
class ReplyBlockTokens {
  const ReplyBlockTokens({
    required this.padding,
    required this.radius,
    required this.accentWidth,
    required this.accentGap,
    required this.senderGap,
    required this.iconSize,
    required this.iconGap,
    required this.iconTopOffset,
    required this.thumbSize,
    required this.thumbRadius,
  });

  /// Inset between the fill and the quoted text. Applies only where the block
  /// carries a fill: unfilled, the accent rule is the block's leading edge.
  final EdgeInsets padding;

  final double radius;

  /// The rule down the leading edge, which is what marks the text as quoted.
  final double accentWidth;

  /// Gap between the accent rule and the quoted text.
  final double accentGap;

  /// Gap between the sender's name and the preview under it.
  final double senderGap;

  final double iconSize;

  /// Gap between the quoted text and the jump indicator.
  final double iconGap;

  /// Drops the jump indicator onto the optical centre of the sender line,
  /// which the glyph's own box doesn't land on.
  final double iconTopOffset;

  /// The still of a quoted picture, which trails the text where the jump
  /// indicator otherwise would.
  final double thumbSize;
  final double thumbRadius;

  static const ReplyBlockTokens standard = ReplyBlockTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    radius: CornerRadius.px8,
    accentWidth: StrokeWidth.px1,
    accentGap: S.s12,
    senderGap: S.s2,
    iconSize: S.s12,
    iconGap: S.s8,
    iconTopOffset: 6,
    thumbSize: S.s32,
    thumbRadius: CornerRadius.px4,
  );

  /// Lines of the quoted message shown before it ellipsizes. Two: enough to
  /// recognize the message, few enough that the quote stays smaller than the
  /// reply to it.
  static const int previewMaxLines = 2;
}
