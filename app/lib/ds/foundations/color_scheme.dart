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
    primary: Primitives.neutral(NeutralShade.s1000),
    secondary: Primitives.neutral(NeutralShade.s700),
    tertiary: Primitives.neutral(NeutralShade.s400),
    quaternary: Primitives.neutral(NeutralShade.s150),
  ),
  backgroundBase: BackgroundBase(
    primary: Primitives.neutral(NeutralShade.s0),
    secondary: Primitives.neutral(NeutralShade.s100),
    tertiary: Primitives.neutral(NeutralShade.s0),
    quaternary: Primitives.neutral(NeutralShade.s150),
    quinary: Primitives.neutral(NeutralShade.s0),
  ),
  backgroundElevated: BackgroundElevated(
    primary: Primitives.neutral(NeutralShade.s0),
    secondary: Primitives.neutral(NeutralShade.s100),
    tertiary: Primitives.neutral(NeutralShade.s0),
    quaternary: Primitives.neutral(NeutralShade.s150),
    quinary: Primitives.neutral(NeutralShade.s100),
  ),
  backgroundMaterial: BackgroundMaterial(
    primary: Primitives.neutral(
      NeutralShade.s0,
    ).withValues(alpha: _materialFillAlpha),
    secondary: Primitives.neutral(
      NeutralShade.s50,
    ).withValues(alpha: _materialFillAlpha),
    tertiary: Primitives.neutral(
      NeutralShade.s0,
    ).withValues(alpha: _materialFillAlpha),
    quaternary: Primitives.neutral(
      NeutralShade.s100,
    ).withValues(alpha: _materialFillAlpha),
  ),
  text: TextColors(
    primary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.94),
    secondary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.85),
    tertiary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.60),
    quaternary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.40),
  ),
  separator: SeparatorColors(
    primary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.10),
    secondary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.10),
  ),
  fill: FillColors(
    primary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.15),
    secondary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.10),
    tertiary: Primitives.neutral(NeutralShade.s900).withValues(alpha: 0.05),
  ),
  function: FunctionColors(
    neutral: FunctionNeutral(
      white: Primitives.neutral(NeutralShade.s0),
      black: Primitives.neutral(NeutralShade.s1000),
      toggleWhite: Primitives.neutral(NeutralShade.s0),
      toggleBlack: Primitives.neutral(NeutralShade.s1000),
      scrim: Primitives.neutral(NeutralShade.s1000).withValues(alpha: 0.20),
      scrimDark: Primitives.neutral(NeutralShade.s1000).withValues(alpha: 0.90),
    ),
    link: Primitives.chromatic(Hue.blue, Shade.s400),
    danger: Primitives.chromatic(Hue.red, Shade.s400),
    success: Primitives.chromatic(Hue.green, Shade.s400),
    warning: FunctionWarning(
      primary: Primitives.chromatic(Hue.yellow, Shade.s400),
      secondary: Primitives.chromatic(Hue.yellow, Shade.s100),
    ),
  ),
  message: MessageColors(
    selfBackground: Primitives.neutral(NeutralShade.s150),
    otherBackground: Primitives.neutral(NeutralShade.s100),
    selfText: Primitives.neutral(NeutralShade.s1000),
    otherText: Primitives.neutral(NeutralShade.s1000),
    selfListPrefix: Primitives.neutral(NeutralShade.s800),
    otherListPrefix: Primitives.neutral(NeutralShade.s800),
    selfQuoteBorder: Primitives.chromatic(Hue.blue, Shade.s500),
    otherQuoteBorder: Primitives.chromatic(Hue.blue, Shade.s500),
    selfQuoteBackground: Primitives.chromatic(Hue.blue, Shade.s50),
    otherQuoteBackground: Primitives.chromatic(Hue.blue, Shade.s50),
    selfTableBorder: Primitives.neutral(NeutralShade.s300),
    otherTableBorder: Primitives.neutral(NeutralShade.s300),
    selfCheckboxBorder: Primitives.neutral(NeutralShade.s400),
    otherCheckboxBorder: Primitives.neutral(NeutralShade.s400),
    selfCheckboxFill: Primitives.neutral(NeutralShade.s200),
    otherCheckboxFill: Primitives.neutral(NeutralShade.s200),
    selfCheckboxCheck: Primitives.neutral(NeutralShade.s1000),
    otherCheckboxCheck: Primitives.neutral(NeutralShade.s1000),
    selfEditedLabel: Primitives.neutral(NeutralShade.s600),
    otherEditedLabel: Primitives.neutral(NeutralShade.s600),
  ),
);

final CustomColorScheme darkCustomColorScheme = CustomColorScheme(
  accentBrand: AccentBrand(
    primary: Primitives.neutral(NeutralShade.s0),
    secondary: Primitives.neutral(NeutralShade.s300),
    tertiary: Primitives.neutral(NeutralShade.s600),
    quaternary: Primitives.neutral(NeutralShade.s850),
  ),
  backgroundBase: BackgroundBase(
    primary: Primitives.neutral(NeutralShade.s1000),
    secondary: Primitives.neutral(NeutralShade.s900),
    tertiary: Primitives.neutral(NeutralShade.s1000),
    quaternary: Primitives.neutral(NeutralShade.s850),
    quinary: Primitives.neutral(NeutralShade.s900),
  ),
  backgroundElevated: BackgroundElevated(
    primary: Primitives.neutral(NeutralShade.s850),
    secondary: Primitives.neutral(NeutralShade.s800),
    tertiary: Primitives.neutral(NeutralShade.s700),
    quaternary: Primitives.neutral(NeutralShade.s600),
    quinary: Primitives.neutral(NeutralShade.s900),
  ),
  backgroundMaterial: BackgroundMaterial(
    primary: Primitives.neutral(
      NeutralShade.s1000,
    ).withValues(alpha: _materialFillAlpha),
    secondary: Primitives.neutral(
      NeutralShade.s950,
    ).withValues(alpha: _materialFillAlpha),
    tertiary: Primitives.neutral(
      NeutralShade.s900,
    ).withValues(alpha: _materialFillAlpha),
    quaternary: Primitives.neutral(
      NeutralShade.s900,
    ).withValues(alpha: _materialFillAlpha),
  ),
  text: TextColors(
    primary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.94),
    secondary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.85),
    tertiary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.60),
    quaternary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.40),
  ),
  separator: SeparatorColors(
    primary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.20),
    secondary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.10),
  ),
  fill: FillColors(
    primary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.20),
    secondary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.15),
    tertiary: Primitives.neutral(NeutralShade.s0).withValues(alpha: 0.10),
  ),
  function: FunctionColors(
    neutral: FunctionNeutral(
      white: Primitives.neutral(NeutralShade.s0),
      black: Primitives.neutral(NeutralShade.s1000),
      toggleWhite: Primitives.neutral(NeutralShade.s1000),
      toggleBlack: Primitives.neutral(NeutralShade.s0),
      scrim: Primitives.neutral(NeutralShade.s1000).withValues(alpha: 0.80),
      scrimDark: Primitives.neutral(NeutralShade.s1000).withValues(alpha: 0.90),
    ),
    link: Primitives.chromatic(Hue.blue, Shade.s500),
    danger: Primitives.chromatic(Hue.red, Shade.s500),
    success: Primitives.chromatic(Hue.green, Shade.s500),
    warning: FunctionWarning(
      primary: Primitives.chromatic(Hue.yellow, Shade.s500),
      secondary: Primitives.chromatic(Hue.yellow, Shade.s900),
    ),
  ),
  message: MessageColors(
    selfBackground: Primitives.neutral(NeutralShade.s850),
    otherBackground: Primitives.neutral(NeutralShade.s900),
    selfText: Primitives.neutral(NeutralShade.s0),
    otherText: Primitives.neutral(NeutralShade.s0),
    selfListPrefix: Primitives.neutral(NeutralShade.s200),
    otherListPrefix: Primitives.neutral(NeutralShade.s200),
    selfQuoteBorder: Primitives.chromatic(Hue.blue, Shade.s600),
    otherQuoteBorder: Primitives.chromatic(Hue.blue, Shade.s600),
    selfQuoteBackground: Primitives.chromatic(Hue.blue, Shade.s800),
    otherQuoteBackground: Primitives.chromatic(Hue.blue, Shade.s800),
    selfTableBorder: Primitives.neutral(NeutralShade.s800),
    otherTableBorder: Primitives.neutral(NeutralShade.s800),
    selfCheckboxBorder: Primitives.neutral(NeutralShade.s600),
    otherCheckboxBorder: Primitives.neutral(NeutralShade.s600),
    selfCheckboxFill: Primitives.neutral(NeutralShade.s700),
    otherCheckboxFill: Primitives.neutral(NeutralShade.s700),
    selfCheckboxCheck: Primitives.neutral(NeutralShade.s0),
    otherCheckboxCheck: Primitives.neutral(NeutralShade.s0),
    selfEditedLabel: Primitives.neutral(NeutralShade.s400),
    otherEditedLabel: Primitives.neutral(NeutralShade.s400),
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
