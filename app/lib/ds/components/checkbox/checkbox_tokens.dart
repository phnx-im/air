// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';

/// Layout tokens for the checkbox.
///
/// Geometry, motion, and the disabled dim tiers only: colors come from the
/// palette at paint time.
abstract final class CheckboxTokens {
  /// Side of the square box.
  ///
  /// One size for both densities: the box already sits at the floor of what
  /// stays legible, and its host row supplies the tap target.
  static const double size = S.s20;

  static const double radius = CornerRadius.px4;

  /// Outline stroke, drawn only while unchecked.
  static const double borderWidth = StrokeWidth.px2;

  /// Side of the check glyph inside the box.
  static const double checkSize = S.s12;

  /// Timing of the checked / unchecked transition.
  static const MotionPreset motion = MotionPreset.short;

  // Disabled recedes by tier rather than as one flat dim, so the box still
  // reads: the check keeps most of its ink, the outline steps back, the fill
  // steps back furthest.

  static const double disabledCheckAlpha = Alpha.a80;
  static const double disabledBorderAlpha = Alpha.a50;
  static const double disabledFillAlpha = Alpha.a20;
}
