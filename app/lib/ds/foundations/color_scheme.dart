// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ds/foundations/primitives.dart';
import 'package:air/ds/foundations/semantic_colors.dart';

/// Alpha applied to every material fill, so a blurred surface still masks the
/// content moving behind it.
const double _materialFillAlpha = 0.5;

class CustomColorScheme {
  final AccentBrand accentBrand;
  final BackgroundBase backgroundBase;
  final BackgroundElevated backgroundElevated;
  final BackgroundMaterial backgroundMaterial;
  final TextColors text;
  final SeparatorColors separator;
  final FillColors fill;
  final FunctionColors function;
  final MessageColors message;

  const CustomColorScheme({
    required this.accentBrand,
    required this.backgroundBase,
    required this.backgroundElevated,
    required this.backgroundMaterial,
    required this.text,
    required this.separator,
    required this.fill,
    required this.function,
    required this.message,
  });

  static CustomColorScheme of(BuildContext context) {
    return MediaQuery.platformBrightnessOf(context) == Brightness.dark
        ? darkCustomColorScheme
        : lightCustomColorScheme;
  }
}

final CustomColorScheme lightCustomColorScheme = CustomColorScheme(
  accentBrand: AccentBrand(
    primary: AppColors.neutral[1000]!,
    secondary: AppColors.neutral[700]!,
    tertiary: AppColors.neutral[400]!,
    quaternary: AppColors.neutral[150]!,
  ),
  backgroundBase: BackgroundBase(
    primary: AppColors.neutral[0]!,
    secondary: AppColors.neutral[100]!,
    tertiary: AppColors.neutral[0]!,
    quaternary: AppColors.neutral[150]!,
    quinary: AppColors.neutral[0]!,
  ),
  backgroundElevated: BackgroundElevated(
    primary: AppColors.neutral[0]!,
    secondary: AppColors.neutral[100]!,
    tertiary: AppColors.neutral[0]!,
    quaternary: AppColors.neutral[150]!,
    quinary: AppColors.neutral[100]!,
  ),
  backgroundMaterial: BackgroundMaterial(
    primary: AppColors.neutral[0]!.withValues(alpha: _materialFillAlpha),
    secondary: AppColors.neutral[50]!.withValues(alpha: _materialFillAlpha),
    tertiary: AppColors.neutral[0]!.withValues(alpha: _materialFillAlpha),
    quaternary: AppColors.neutral[100]!.withValues(alpha: _materialFillAlpha),
  ),
  text: TextColors(
    primary: AppColors.neutral[900]!.withValues(alpha: 0.94),
    secondary: AppColors.neutral[900]!.withValues(alpha: 0.85),
    tertiary: AppColors.neutral[900]!.withValues(alpha: 0.60),
    quaternary: AppColors.neutral[900]!.withValues(alpha: 0.40),
  ),
  separator: SeparatorColors(
    primary: AppColors.neutral[900]!.withValues(alpha: 0.10),
    secondary: AppColors.neutral[900]!.withValues(alpha: 0.10),
  ),
  fill: FillColors(
    primary: AppColors.neutral[900]!.withValues(alpha: 0.15),
    secondary: AppColors.neutral[900]!.withValues(alpha: 0.10),
    tertiary: AppColors.neutral[900]!.withValues(alpha: 0.05),
  ),
  function: FunctionColors(
    neutral: FunctionNeutral(
      white: AppColors.neutral[0]!,
      black: AppColors.neutral[1000]!,
      toggleWhite: AppColors.neutral[0]!,
      toggleBlack: AppColors.neutral[1000]!,
      scrim: AppColors.neutral[1000]!.withValues(alpha: 0.20),
      scrimDark: AppColors.neutral[1000]!.withValues(alpha: 0.90),
    ),
    link: AppColors.blue[400]!,
    danger: AppColors.red[400]!,
    success: AppColors.green[400]!,
    warning: FunctionWarning(
      primary: AppColors.yellow[400]!,
      secondary: AppColors.yellow[100]!,
    ),
  ),
  message: MessageColors(
    selfBackground: AppColors.neutral[150]!,
    otherBackground: AppColors.neutral[100]!,
    selfText: AppColors.neutral[1000]!,
    otherText: AppColors.neutral[1000]!,
    selfListPrefix: AppColors.neutral[800]!,
    otherListPrefix: AppColors.neutral[800]!,
    selfQuoteBorder: AppColors.blue[500]!,
    otherQuoteBorder: AppColors.blue[500]!,
    selfQuoteBackground: AppColors.blue[50]!,
    otherQuoteBackground: AppColors.blue[50]!,
    selfTableBorder: AppColors.neutral[300]!,
    otherTableBorder: AppColors.neutral[300]!,
    selfCheckboxBorder: AppColors.neutral[400]!,
    otherCheckboxBorder: AppColors.neutral[400]!,
    selfCheckboxFill: AppColors.neutral[200]!,
    otherCheckboxFill: AppColors.neutral[200]!,
    selfCheckboxCheck: AppColors.neutral[1000]!,
    otherCheckboxCheck: AppColors.neutral[1000]!,
    selfEditedLabel: AppColors.neutral[600]!,
    otherEditedLabel: AppColors.neutral[600]!,
  ),
);

final CustomColorScheme darkCustomColorScheme = CustomColorScheme(
  accentBrand: AccentBrand(
    primary: AppColors.neutral[0]!,
    secondary: AppColors.neutral[300]!,
    tertiary: AppColors.neutral[600]!,
    quaternary: AppColors.neutral[850]!,
  ),
  backgroundBase: BackgroundBase(
    primary: AppColors.neutral[1000]!,
    secondary: AppColors.neutral[900]!,
    tertiary: AppColors.neutral[1000]!,
    quaternary: AppColors.neutral[850]!,
    quinary: AppColors.neutral[900]!,
  ),
  backgroundElevated: BackgroundElevated(
    primary: AppColors.neutral[850]!,
    secondary: AppColors.neutral[800]!,
    tertiary: AppColors.neutral[700]!,
    quaternary: AppColors.neutral[600]!,
    quinary: AppColors.neutral[900]!,
  ),
  backgroundMaterial: BackgroundMaterial(
    primary: AppColors.neutral[1000]!.withValues(alpha: _materialFillAlpha),
    secondary: AppColors.neutral[950]!.withValues(alpha: _materialFillAlpha),
    tertiary: AppColors.neutral[900]!.withValues(alpha: _materialFillAlpha),
    quaternary: AppColors.neutral[900]!.withValues(alpha: _materialFillAlpha),
  ),
  text: TextColors(
    primary: AppColors.neutral[0]!.withValues(alpha: 0.94),
    secondary: AppColors.neutral[0]!.withValues(alpha: 0.85),
    tertiary: AppColors.neutral[0]!.withValues(alpha: 0.60),
    quaternary: AppColors.neutral[0]!.withValues(alpha: 0.40),
  ),
  separator: SeparatorColors(
    primary: AppColors.neutral[0]!.withValues(alpha: 0.20),
    secondary: AppColors.neutral[0]!.withValues(alpha: 0.10),
  ),
  fill: FillColors(
    primary: AppColors.neutral[0]!.withValues(alpha: 0.20),
    secondary: AppColors.neutral[0]!.withValues(alpha: 0.15),
    tertiary: AppColors.neutral[0]!.withValues(alpha: 0.10),
  ),
  function: FunctionColors(
    neutral: FunctionNeutral(
      white: AppColors.neutral[0]!,
      black: AppColors.neutral[1000]!,
      toggleWhite: AppColors.neutral[1000]!,
      toggleBlack: AppColors.neutral[0]!,
      scrim: AppColors.neutral[1000]!.withValues(alpha: 0.80),
      scrimDark: AppColors.neutral[1000]!.withValues(alpha: 0.90),
    ),
    link: AppColors.blue[500]!,
    danger: AppColors.red[500]!,
    success: AppColors.green[500]!,
    warning: FunctionWarning(
      primary: AppColors.yellow[500]!,
      secondary: AppColors.yellow[900]!,
    ),
  ),
  message: MessageColors(
    selfBackground: AppColors.neutral[850]!,
    otherBackground: AppColors.neutral[900]!,
    selfText: AppColors.neutral[0]!,
    otherText: AppColors.neutral[0]!,
    selfListPrefix: AppColors.neutral[200]!,
    otherListPrefix: AppColors.neutral[200]!,
    selfQuoteBorder: AppColors.blue[600]!,
    otherQuoteBorder: AppColors.blue[600]!,
    selfQuoteBackground: AppColors.blue[800]!,
    otherQuoteBackground: AppColors.blue[800]!,
    selfTableBorder: AppColors.neutral[800]!,
    otherTableBorder: AppColors.neutral[800]!,
    selfCheckboxBorder: AppColors.neutral[600]!,
    otherCheckboxBorder: AppColors.neutral[600]!,
    selfCheckboxFill: AppColors.neutral[700]!,
    otherCheckboxFill: AppColors.neutral[700]!,
    selfCheckboxCheck: AppColors.neutral[0]!,
    otherCheckboxCheck: AppColors.neutral[0]!,
    selfEditedLabel: AppColors.neutral[400]!,
    otherEditedLabel: AppColors.neutral[400]!,
  ),
);

final ColorScheme lightColorScheme = ColorScheme(
  brightness: Brightness.light,
  primary: lightCustomColorScheme.text.primary,
  onPrimary: lightCustomColorScheme.backgroundBase.primary,
  secondary: lightCustomColorScheme.text.secondary,
  onSecondary: lightCustomColorScheme.backgroundBase.primary,
  surface: lightCustomColorScheme.backgroundBase.primary,
  onSurface: lightCustomColorScheme.text.primary,
  error: lightCustomColorScheme.function.danger,
  onError: lightCustomColorScheme.text.primary,
);

final ColorScheme darkColorScheme = ColorScheme(
  brightness: Brightness.dark,
  primary: darkCustomColorScheme.text.primary,
  onPrimary: darkCustomColorScheme.backgroundBase.primary,
  secondary: darkCustomColorScheme.text.secondary,
  onSecondary: darkCustomColorScheme.backgroundBase.primary,
  surface: darkCustomColorScheme.backgroundBase.primary,
  onSurface: darkCustomColorScheme.text.primary,
  error: darkCustomColorScheme.function.danger,
  onError: darkCustomColorScheme.text.primary,
);
