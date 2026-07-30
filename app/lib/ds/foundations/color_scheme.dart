// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ds/foundations/primitives.dart';
import 'package:air/ds/foundations/semantic_colors.dart';

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

  /// Group every semantic slot into bundles for one brightness. The alias
  /// table owns the values, so this only decides which bundle a slot lands
  /// in.
  factory CustomColorScheme.resolve(Brightness brightness) {
    Color r(SemanticColor slot) => slot.resolve(brightness);
    return CustomColorScheme(
      accentBrand: AccentBrand(
        primary: r(SemanticColor.accentBrandPrimary),
        secondary: r(SemanticColor.accentBrandSecondary),
        tertiary: r(SemanticColor.accentBrandTertiary),
        quaternary: r(SemanticColor.accentBrandQuaternary),
      ),
      backgroundBase: BackgroundBase(
        primary: r(SemanticColor.backgroundBasePrimary),
        secondary: r(SemanticColor.backgroundBaseSecondary),
        tertiary: r(SemanticColor.backgroundBaseTertiary),
        quaternary: r(SemanticColor.backgroundBaseQuaternary),
        quinary: r(SemanticColor.backgroundBaseQuinary),
      ),
      backgroundElevated: BackgroundElevated(
        primary: r(SemanticColor.backgroundElevatedPrimary),
        secondary: r(SemanticColor.backgroundElevatedSecondary),
        tertiary: r(SemanticColor.backgroundElevatedTertiary),
        quaternary: r(SemanticColor.backgroundElevatedQuaternary),
        quinary: r(SemanticColor.backgroundElevatedQuinary),
      ),
      backgroundMaterial: BackgroundMaterial(
        primary: r(SemanticColor.backgroundMaterialPrimary),
        secondary: r(SemanticColor.backgroundMaterialSecondary),
        tertiary: r(SemanticColor.backgroundMaterialTertiary),
        quaternary: r(SemanticColor.backgroundMaterialQuaternary),
      ),
      text: TextColors(
        primary: r(SemanticColor.textPrimary),
        secondary: r(SemanticColor.textSecondary),
        tertiary: r(SemanticColor.textTertiary),
        quaternary: r(SemanticColor.textQuaternary),
      ),
      separator: SeparatorColors(
        primary: r(SemanticColor.separatorPrimary),
        secondary: r(SemanticColor.separatorSecondary),
      ),
      fill: FillColors(
        primary: r(SemanticColor.fillPrimary),
        secondary: r(SemanticColor.fillSecondary),
        tertiary: r(SemanticColor.fillTertiary),
      ),
      function: FunctionColors(
        neutral: FunctionNeutral(
          white: r(SemanticColor.functionNeutralWhite),
          black: r(SemanticColor.functionNeutralBlack),
          toggleWhite: r(SemanticColor.functionNeutralToggleWhite),
          toggleBlack: r(SemanticColor.functionNeutralToggleBlack),
          scrim: r(SemanticColor.functionNeutralScrim),
          scrimDark: r(SemanticColor.functionNeutralScrimDark),
        ),
        link: r(SemanticColor.functionLinkPrimary),
        danger: r(SemanticColor.functionDangerPrimary),
        success: FunctionSuccess(
          primary: r(SemanticColor.functionSuccessPrimary),
          secondary: r(SemanticColor.functionSuccessSecondary),
        ),
        warning: FunctionWarning(
          primary: r(SemanticColor.functionWarningPrimary),
          secondary: r(SemanticColor.functionWarningSecondary),
        ),
      ),
      message: brightness == Brightness.dark
          ? _darkMessageColors
          : _lightMessageColors,
    );
  }

  static CustomColorScheme of(BuildContext context) {
    return MediaQuery.platformBrightnessOf(context) == Brightness.dark
        ? darkCustomColorScheme
        : lightCustomColorScheme;
  }
}

final CustomColorScheme lightCustomColorScheme = CustomColorScheme.resolve(
  Brightness.light,
);

final CustomColorScheme darkCustomColorScheme = CustomColorScheme.resolve(
  Brightness.dark,
);

/// Message-bubble colors are the one bundle without aliases: they are an
/// Air-specific extension that the reference DS carries in its message-bubble
/// pattern tokens rather than in the semantic palette.
final MessageColors _lightMessageColors = MessageColors(
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
);

final MessageColors _darkMessageColors = MessageColors(
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
