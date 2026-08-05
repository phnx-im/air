// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the quick-reaction bar, per density.
///
/// Geometry only: colors come from the palette at paint time.
@immutable
class ReactionBarTokens {
  const ReactionBarTokens({
    required this.itemSize,
    required this.glyphSize,
    required this.moreIconSize,
  });

  /// Square tap target around each emoji. The glyphs are small enough that the
  /// target, not the glyph, is what sets the bar's rhythm, so no gap sits
  /// between two items.
  final double itemSize;

  final double glyphSize;

  final double moreIconSize;

  /// Inset inside the floating pill.
  static const EdgeInsets containerPadding = EdgeInsets.symmetric(
    horizontal: S.s8,
    vertical: S.s4,
  );

  /// Diameter of the trailing button that escalates to the full picker, and of
  /// its glyph. The button keeps [itemSize] as its hit target so it's no
  /// harder to hit than an emoji.
  static const double moreSize = ButtonIconSize.s32;

  static const double radius = CornerRadius.full;

  /// Elevation of the floating pill.
  static const Elevation elevation = Elevation.medium;

  /// Vertical distance the bar keeps from whatever it's anchored to.
  static const double anchorGap = S.s12;

  /// Inset the bar keeps from the edges of the screen, on top of the safe area.
  static const double screenInset = S.s8;

  /// A picked emoji pulses out to this scale and settles back, so the choice
  /// registers while the bar is still fading out.
  static const double pickScale = 1.25;
  static const MotionPreset pickMotion = MotionPreset.short;

  /// Weights of the pulse's two halves. Growing faster than it settles makes
  /// the pick read as a response rather than a bounce.
  static const int pickGrowWeight = 40;
  static const int pickSettleWeight = 60;

  /// The touch target is the platform minimum rather than a scale step: a
  /// finger on a bar of six adjacent emojis needs every pixel of it.
  static const ReactionBarTokens phone = ReactionBarTokens(
    itemSize: 44,
    glyphSize: S.s28,
    moreIconSize: S.s20,
  );

  /// Tighter than [phone]: a pointer hits a smaller target reliably, and the
  /// glyph follows the smaller desktop type scale.
  static const ReactionBarTokens desktop = ReactionBarTokens(
    itemSize: S.s32,
    glyphSize: S.s24,
    moreIconSize: S.s16,
  );

  static ReactionBarTokens get current => DeviceType.isPhone ? phone : desktop;
}
