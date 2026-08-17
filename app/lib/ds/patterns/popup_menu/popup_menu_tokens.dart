// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';

/// Layout and motion tokens for the floating shell `showOverlayMenu` puts a
/// menu in.
///
/// The shell wraps a `Menu`, so the card's own chrome stays in `MenuTokens`.
/// What's left is where the card sits relative to its anchor and to the
/// viewport, and how it arrives.
abstract final class PopupMenuTokens {
  /// Gap the card keeps from the viewport's safe edges.
  static const double edgeInset = S.s8;

  /// Gap between the anchor and the card's near edge.
  static const double anchorGap = S.s8;

  /// How far the card travels as it slides into place, from the anchor it hangs
  /// off toward its resting spot. Motion is single-axis: a menu opening
  /// downward slides down and one opening upward slides up, never diagonally.
  static const double slideDistance = S.s40;

  static const MotionPreset enter = MotionPreset.short;
}
