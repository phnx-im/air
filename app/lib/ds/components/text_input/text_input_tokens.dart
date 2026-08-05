// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a text input, per density.
///
/// Geometry only: colors come from the palette and text styles from the
/// typescale at paint time.
@immutable
class AppTextInputTokens {
  const AppTextInputTokens({
    required this.radius,
    required this.fieldPadding,
    required this.borderWidth,
    required this.labelGap,
    required this.helperGap,
    required this.labelPadding,
    required this.helperPadding,
  });

  final double radius;

  /// Inset between the field's edge and the text it carries.
  final EdgeInsets fieldPadding;

  /// Width of the outline the field wears while it shows an error.
  final double borderWidth;

  /// Gap below the label, and above the helper / error line.
  final double labelGap;
  final double helperGap;

  /// Horizontal inset for the label and the helper / error line. The field
  /// carries its own padding, so these only line the two texts up under it.
  final EdgeInsets labelPadding;
  final EdgeInsets helperPadding;

  static const AppTextInputTokens phone = AppTextInputTokens(
    radius: CornerRadius.px12,
    fieldPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
    borderWidth: StrokeWidth.px1,
    labelGap: S.s8,
    helperGap: S.s8,
    labelPadding: EdgeInsets.only(left: S.s8),
    helperPadding: EdgeInsets.only(left: S.s8),
  );

  /// Denser than [phone]: a pointer aims at the caret directly, so the field
  /// needs less room around the text than a fingertip does.
  static const AppTextInputTokens desktop = AppTextInputTokens(
    radius: CornerRadius.px12,
    fieldPadding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    borderWidth: StrokeWidth.px1,
    labelGap: S.s8,
    helperGap: S.s8,
    labelPadding: EdgeInsets.only(left: S.s8),
    helperPadding: EdgeInsets.only(left: S.s8),
  );

  static AppTextInputTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
