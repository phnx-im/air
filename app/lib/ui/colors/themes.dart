// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ui/colors/palette.dart';
import 'package:air/ui/colors/semantic.dart';

class CustomColorScheme {
  final BackGroundBaseColors backgroundBase;
  final BackGroundElevatedColors backgroundElevated;
  final TextColors text;
  final SeparatorColors separator;
  final FillColors fill;
  final FunctionColors function;
  final MessageColors message;
  final AccentColors accent;

  CustomColorScheme({
    required this.backgroundBase,
    required this.backgroundElevated,
    required this.text,
    required this.separator,
    required this.fill,
    required this.function,
    required this.message,
    required this.accent,
  });

  static CustomColorScheme of(BuildContext context) {
    return MediaQuery.platformBrightnessOf(context) == Brightness.dark
        ? darkCustomColorScheme
        : lightCustomColorScheme;
  }
}

final CustomColorScheme lightCustomColorScheme = CustomColorScheme(
  backgroundBase: BackGroundBaseColors(
    primary: AppColors.neutral[0]!,
    secondary: AppColors.neutral[50]!,
    tertiary: AppColors.neutral[0]!,
    quaternary: AppColors.neutral[50]!,
  ),
  backgroundElevated: BackGroundElevatedColors(
    primary: AppColors.neutral[0]!,
    secondary: AppColors.neutral[50]!,
    tertiary: AppColors.neutral[0]!,
    quaternary: AppColors.neutral[100]!,
  ),
  text: TextColors(
    primary: AppColors.neutral[950]!.withValues(alpha: 0.95),
    secondary: AppColors.neutral[950]!.withValues(alpha: 0.85),
    tertiary: AppColors.neutral[950]!.withValues(alpha: 0.60),
    quaternary: AppColors.neutral[950]!.withValues(alpha: 0.40),
  ),
  separator: SeparatorColors(
    primary: AppColors.neutral[950]!.withValues(alpha: 0.20),
    secondary: AppColors.neutral[950]!.withValues(alpha: 0.10),
  ),
  fill: FillColors(
    primary: AppColors.neutral[950]!.withValues(alpha: 0.15),
    secondary: AppColors.neutral[950]!.withValues(alpha: 0.10),
    tertiary: AppColors.neutral[950]!.withValues(alpha: 0.05),
  ),
  function: FunctionColors(
    white: AppColors.neutral[0]!,
    black: AppColors.neutral[1000]!,
    toggleWhite: AppColors.neutral[0]!,
    toggleBlack: AppColors.neutral[1000]!,
    success: AppColors.green[400]!,
    warning: AppColors.yellow[400]!,
    danger: AppColors.red[400]!,
    link: AppColors.blue[400]!,
    selfLink: AppColors.blue[150]!,
  ),
  message: MessageColors(
    selfBackground: AppColors.neutral[600]!,
    otherBackground: AppColors.neutral[50]!,
    selfText: AppColors.neutral[0]!,
    otherText: AppColors.neutral[1000]!,
    selfListPrefix: AppColors.neutral[200]!,
    otherListPrefix: AppColors.neutral[800]!,
    selfQuoteBorder: AppColors.blue[400]!,
    otherQuoteBorder: AppColors.blue[500]!,
    selfQuoteBackground: AppColors.blue[700]!,
    otherQuoteBackground: AppColors.blue[50]!,
    selfTableBorder: AppColors.neutral[300]!,
    otherTableBorder: AppColors.neutral[300]!,
    selfCheckboxBorder: AppColors.neutral[200]!,
    otherCheckboxBorder: AppColors.neutral[400]!,
    selfCheckboxFill: AppColors.neutral[800]!,
    otherCheckboxFill: AppColors.neutral[200]!,
    selfCheckboxCheck: AppColors.neutral[0]!,
    otherCheckboxCheck: AppColors.neutral[1000]!,
    selfEditedLabel: AppColors.neutral[400]!,
    otherEditedLabel: AppColors.neutral[600]!,
  ),
  accent: AccentColors(
    primary: AppColors.neutral[1000]!,
    secondary: AppColors.neutral[700]!,
    tertiary: AppColors.neutral[400]!,
    quaternary: AppColors.neutral[100]!,
  ),
);

final CustomColorScheme darkCustomColorScheme = CustomColorScheme(
  backgroundBase: BackGroundBaseColors(
    primary: AppColors.neutral[1000]!,
    secondary: AppColors.neutral[950]!,
    tertiary: AppColors.neutral[1000]!,
    quaternary: AppColors.neutral[900]!,
  ),
  backgroundElevated: BackGroundElevatedColors(
    primary: AppColors.neutral[900]!,
    secondary: AppColors.neutral[800]!,
    tertiary: AppColors.neutral[700]!,
    quaternary: AppColors.neutral[600]!,
  ),
  text: TextColors(
    primary: AppColors.neutral[0]!.withValues(alpha: 0.95),
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
    white: AppColors.neutral[0]!,
    black: AppColors.neutral[1000]!,
    toggleWhite: AppColors.neutral[1000]!,
    toggleBlack: AppColors.neutral[0]!,
    success: AppColors.green[500]!,
    warning: AppColors.yellow[500]!,
    danger: AppColors.red[500]!,
    link: AppColors.blue[200]!,
    selfLink: AppColors.blue[600]!,
  ),
  message: MessageColors(
    selfBackground: AppColors.neutral[300]!,
    otherBackground: AppColors.neutral[900]!,
    selfText: AppColors.neutral[900]!,
    otherText: AppColors.neutral[100]!,
    selfListPrefix: AppColors.neutral[800]!,
    otherListPrefix: AppColors.neutral[200]!,
    selfQuoteBorder: AppColors.blue[300]!,
    otherQuoteBorder: AppColors.blue[600]!,
    selfQuoteBackground: AppColors.blue[200]!,
    otherQuoteBackground: AppColors.blue[800]!,
    selfTableBorder: AppColors.neutral[600]!,
    otherTableBorder: AppColors.neutral[600]!,
    selfCheckboxBorder: AppColors.neutral[800]!,
    otherCheckboxBorder: AppColors.neutral[600]!,
    selfCheckboxFill: AppColors.neutral[400]!,
    otherCheckboxFill: AppColors.neutral[700]!,
    selfCheckboxCheck: AppColors.neutral[1000]!,
    otherCheckboxCheck: AppColors.neutral[0]!,
    selfEditedLabel: AppColors.neutral[600]!,
    otherEditedLabel: AppColors.neutral[400]!,
  ),
  accent: AccentColors(
    primary: AppColors.neutral[0]!,
    secondary: AppColors.neutral[300]!,
    tertiary: AppColors.neutral[600]!,
    quaternary: AppColors.neutral[900]!,
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
