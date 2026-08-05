// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for the stamp under a message bubble.
///
/// Geometry only: colors come from the palette and the type from the
/// typescale at paint time.
@immutable
class MessageMetaTokens {
  const MessageMetaTokens({
    required this.iconSize,
    required this.gap,
    required this.dotSize,
    required this.dotGap,
    required this.bubbleGap,
    required this.bottomPadding,
    required this.contentOffset,
  });

  /// Delivery glyph. A step below the body's own glyph size: the stamp reports,
  /// it doesn't compete with the message.
  final double iconSize;

  /// Gap between the delivery glyph and its label.
  final double gap;

  /// Separator between the stamp's parts, and the space on either side of it.
  final double dotSize;
  final double dotGap;

  /// Space between the bubble and the stamp.
  final double bubbleGap;

  /// Space below the stamp, toward the next message.
  final double bottomPadding;

  /// Inset on the bubble's own side, so the stamp's text lines up with the
  /// text inside the bubble. Matches the bubble's content inset. A host whose
  /// content runs to the bubble edge passes zero instead.
  final double contentOffset;

  static const MessageMetaTokens standard = MessageMetaTokens(
    iconSize: S.s12,
    gap: S.s4,
    dotSize: S.s4,
    dotGap: S.s8,
    bubbleGap: S.s4,
    bottomPadding: S.s2,
    contentOffset: S.s12,
  );
}
