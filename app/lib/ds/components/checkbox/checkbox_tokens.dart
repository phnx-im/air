// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the checkbox.
///
/// Geometry, motion, and the disabled dim tiers only: colors come from the
/// palette at paint time.
@immutable
class CheckboxTokens {
  const CheckboxTokens({
    required this.size,
    required this.radius,
    required this.borderWidth,
    required this.checkSize,
    required this.motion,
    required this.disabledCheckAlpha,
    required this.disabledBorderAlpha,
    required this.disabledFillAlpha,
  });

  /// Side of the square box.
  final double size;

  final double radius;

  /// Outline stroke, drawn only while unchecked.
  final double borderWidth;

  /// Side of the check glyph inside the box.
  final double checkSize;

  /// Timing of the checked / unchecked transition.
  final MotionPreset motion;

  // Disabled recedes by tier rather than as one flat dim, so the box still
  // reads: the check keeps most of its ink, the outline steps back, the fill
  // steps back furthest.

  final double disabledCheckAlpha;
  final double disabledBorderAlpha;
  final double disabledFillAlpha;

  /// One size for both densities: the box already sits at the floor of what
  /// stays legible, and its host row supplies the tap target.
  static const CheckboxTokens standard = CheckboxTokens(
    size: S.s20,
    radius: CornerRadius.px4,
    borderWidth: StrokeWidth.px2,
    checkSize: S.s12,
    motion: MotionPreset.short,
    disabledCheckAlpha: Alpha.a80,
    disabledBorderAlpha: Alpha.a50,
    disabledFillAlpha: Alpha.a20,
  );
}
