// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout and motion tokens for the floating shell `showOverlayMenu` puts a
/// menu in.
///
/// The shell wraps a `Menu`, so the card's own chrome stays in `MenuTokens`.
/// What's left is where the card sits relative to its anchor and to the
/// viewport, and how it arrives.
@immutable
class PopupMenuTokens {
  const PopupMenuTokens({
    required this.edgeInset,
    required this.anchorGap,
    required this.slideDistance,
    required this.enter,
  });

  /// Gap the card keeps from the viewport's safe edges.
  final double edgeInset;

  /// Gap between the anchor and the card's near edge.
  final double anchorGap;

  /// How far the card travels as it slides into place, from the anchor it hangs
  /// off toward its resting spot. Motion is single-axis: a menu opening
  /// downward slides down and one opening upward slides up, never diagonally.
  final double slideDistance;

  final MotionPreset enter;

  static const PopupMenuTokens standard = PopupMenuTokens(
    edgeInset: S.s8,
    anchorGap: S.s8,
    slideDistance: S.s40,
    enter: MotionPreset.short,
  );
}
