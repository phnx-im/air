// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for the stamp under a message bubble.
///
/// Geometry only: colors come from the palette and the type from the
/// typescale at paint time.
abstract final class MessageMetaTokens {
  /// Delivery glyph. A step below the body's own glyph size: the stamp reports,
  /// it doesn't compete with the message.
  static const double iconSize = S.s12;

  /// Gap between the delivery glyph and its label.
  static const double gap = S.s4;

  /// Separator between the stamp's parts, and the space on either side of it.
  static const double dotSize = S.s4;
  static const double dotGap = S.s8;

  /// Space between the bubble and the stamp.
  static const double bubbleGap = S.s4;

  /// Space below the stamp, toward the next message.
  static const double bottomPadding = S.s2;

  /// Inset on the bubble's own side, so the stamp's text lines up with the
  /// text inside the bubble. Matches the bubble's content inset. A host whose
  /// content runs to the bubble edge passes zero instead.
  static const double contentOffset = S.s12;

  /// The box the stamp takes: its line, whichever of glyph and type runs
  /// taller, plus the space above and below it.
  ///
  /// A stamp is one line by definition, so its height is geometry rather than
  /// a measurement, and a host that has to reserve the space before the stamp
  /// lays out takes this. The stamp sets its type tight, so a line box is the
  /// font size itself.
  static double heightOf(BuildContext context) =>
      bubbleGap +
      math.max(
        iconSize,
        MediaQuery.textScalerOf(context).scale(typeScale.body.mini.fontSize),
      ) +
      bottomPadding;
}
