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
  const AppTextInputTokens({required this.fieldPadding});

  /// Inset between the field's edge and the text it carries.
  final EdgeInsets fieldPadding;

  static const double radius = CornerRadius.px12;

  /// Width of the outline the field wears while it shows an error.
  static const double borderWidth = StrokeWidth.px1;

  /// Gap below the label, and above the helper / error line.
  static const double labelGap = S.s8;
  static const double helperGap = S.s8;

  /// Horizontal inset for the label and the helper / error line. The field
  /// carries its own padding, so these only line the two texts up under it.
  static const EdgeInsets labelPadding = EdgeInsets.only(left: S.s8);
  static const EdgeInsets helperPadding = EdgeInsets.only(left: S.s8);

  static const AppTextInputTokens phone = AppTextInputTokens(
    fieldPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
  );

  /// Denser than [phone]: a pointer aims at the caret directly, so the field
  /// needs less room around the text than a fingertip does.
  static const AppTextInputTokens desktop = AppTextInputTokens(
    fieldPadding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
  );

  static AppTextInputTokens get current => DeviceType.isPhone ? phone : desktop;
}
