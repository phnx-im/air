// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Size tier of an [Button]. The tier carries the density too, so a surface
/// picks one and the geometry follows.
enum ButtonSize {
  small,
  large;

  static ButtonSize get current => DeviceType.isPhone ? large : small;
}

/// Weight of an [Button] in its surface: the one action a surface leads
/// with, or an action standing next to it.
enum ButtonType { primary, secondary }

/// Whether the action is destructive.
enum ButtonTone { normal, danger }

/// Interaction state of an [Button]. Only [active] takes a tap: [disabled]
/// fades its content, [pending] swaps the content for a spinner.
enum ButtonState { active, disabled, pending }

/// Label row of the typescale for a size tier. Resolved at paint time, since
/// the scale follows the platform.
extension ButtonSizeTypography on ButtonSize {
  TypeStyleToken get labelToken => switch (this) {
    ButtonSize.small => typeScale.body.xs,
    ButtonSize.large => typeScale.body.regular,
  };
}

/// Layout tokens for a pill button, per size tier.
///
/// Geometry only: colors come from the palette at paint time, picked by
/// [ButtonType] and [ButtonTone].
@immutable
class ButtonTokens {
  const ButtonTokens({
    required this.height,
    required this.radius,
    required this.padding,
    required this.iconSize,
    required this.iconLabelGap,
    required this.spinnerWidth,
  });

  /// Fixed height, not a floor: the pill keeps one height across a row of
  /// buttons whether or not any of them carries a glyph.
  final double height;

  final double radius;
  final EdgeInsets padding;

  final double iconSize;
  final double iconLabelGap;

  /// Stroke of the [ButtonState.pending] spinner, which takes the glyph's
  /// footprint.
  final double spinnerWidth;

  /// Floor for the tap area on a touch device. The pill paints at its own
  /// height, and a shorter one takes a transparent ring so the target still
  /// fits a finger.
  static const double minTouchHeight = S.s48;

  static const ButtonTokens small = ButtonTokens(
    height: S.s32,
    radius: CornerRadius.px8,
    padding: EdgeInsets.symmetric(horizontal: S.s12),
    iconSize: S.s12,
    iconLabelGap: S.s8,
    spinnerWidth: StrokeWidth.px2,
  );

  static const ButtonTokens large = ButtonTokens(
    height: S.s40,
    radius: CornerRadius.px12,
    padding: EdgeInsets.symmetric(horizontal: S.s16),
    iconSize: S.s16,
    iconLabelGap: S.s8,
    spinnerWidth: StrokeWidth.px2,
  );

  static ButtonTokens of(ButtonSize size) => switch (size) {
    ButtonSize.small => small,
    ButtonSize.large => large,
  };
}
