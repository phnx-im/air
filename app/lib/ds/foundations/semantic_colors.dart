// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ds/foundations/primitives.dart';

class AccentBrand {
  final Color primary, secondary, tertiary, quaternary;

  const AccentBrand({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class BackgroundBase {
  final Color primary, secondary, tertiary, quaternary, quinary;

  const BackgroundBase({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
    required this.quinary,
  });
}

class BackgroundElevated {
  final Color primary, secondary, tertiary, quaternary, quinary;

  const BackgroundElevated({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
    required this.quinary,
  });
}

/// Translucent fills for frosted-glass surfaces. The fill alpha is baked in,
/// so these pair directly with a sigma from `Effect.blur`.
class BackgroundMaterial {
  final Color primary, secondary, tertiary, quaternary;

  const BackgroundMaterial({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class TextPalette {
  final Color primary, secondary, tertiary, quaternary;

  const TextPalette({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class SeparatorPalette {
  final Color primary, secondary;

  const SeparatorPalette({required this.primary, required this.secondary});
}

class FillPalette {
  final Color primary, secondary, tertiary;

  const FillPalette({
    required this.primary,
    required this.secondary,
    required this.tertiary,
  });
}

/// Neutrals that either stay put or flip with the theme. [white] and [black]
/// are for ink on a fixed-color surface (avatar initials, lightbox glass);
/// [toggleWhite] and [toggleBlack] swap between light and dark.
class FunctionNeutral {
  final Color white, black, toggleWhite, toggleBlack, scrim, scrimDark;

  const FunctionNeutral({
    required this.white,
    required this.black,
    required this.toggleWhite,
    required this.toggleBlack,
    required this.scrim,
    required this.scrimDark,
  });
}

class FunctionWarning {
  final Color primary, secondary;

  const FunctionWarning({required this.primary, required this.secondary});
}

class FunctionSuccess {
  final Color primary, secondary;

  const FunctionSuccess({required this.primary, required this.secondary});
}

class FunctionPalette {
  final FunctionNeutral neutral;
  final Color link, danger;
  final FunctionSuccess success;
  final FunctionWarning warning;

  const FunctionPalette({
    required this.neutral,
    required this.link,
    required this.danger,
    required this.success,
    required this.warning,
  });
}

/// Message-bubble colors. An Air-specific extension of the design system:
/// the reference DS carries these in its message-bubble pattern tokens.
class MessagePalette {
  final Color selfBackground, otherBackground;
  final Color selfText, otherText;
  final Color selfListPrefix, otherListPrefix;
  final Color selfQuoteBorder, otherQuoteBorder;
  final Color selfQuoteBackground, otherQuoteBackground;
  final Color selfTableBorder, otherTableBorder;
  final Color selfCheckboxBorder, otherCheckboxBorder;
  final Color selfCheckboxFill, otherCheckboxFill;
  final Color selfCheckboxCheck, otherCheckboxCheck;
  final Color selfEditedLabel, otherEditedLabel;

  const MessagePalette({
    required this.selfBackground,
    required this.otherBackground,
    required this.selfText,
    required this.otherText,
    required this.selfListPrefix,
    required this.otherListPrefix,
    required this.selfQuoteBorder,
    required this.otherQuoteBorder,
    required this.selfQuoteBackground,
    required this.otherQuoteBackground,
    required this.selfTableBorder,
    required this.otherTableBorder,
    required this.selfCheckboxBorder,
    required this.otherCheckboxBorder,
    required this.selfCheckboxFill,
    required this.otherCheckboxFill,
    required this.selfCheckboxCheck,
    required this.otherCheckboxCheck,
    required this.selfEditedLabel,
    required this.otherEditedLabel,
  });
}

/// Alpha applied to every material fill, so a blurred surface still masks the
/// content moving behind it.
const double _materialFillAlpha = 0.5;

/// A primitive cell plus the alpha applied to it. Aliases point at these
/// rather than at finished colors, so a slot records which primitive it names
/// and not just the value that falls out of it.
sealed class TonedRef {
  final double opacity;

  const TonedRef(this.opacity);

  Color resolve() {
    final base = switch (this) {
      NeutralRef(:final shade) => Primitive.neutral(shade),
      ChromaticRef(:final hue, :final shade) => Primitive.chromatic(hue, shade),
    };
    return opacity == 1.0 ? base : base.withValues(alpha: opacity);
  }
}

/// Neutral carries its own shade axis, so it needs a ref of its own.
final class NeutralRef extends TonedRef {
  final NeutralShade shade;

  const NeutralRef(this.shade, [super.opacity = 1.0]);
}

final class ChromaticRef extends TonedRef {
  final Hue hue;
  final Shade shade;

  const ChromaticRef(this.hue, this.shade, [super.opacity = 1.0]);
}

/// The light and dark value behind one semantic slot. Keeping both modes in
/// one place is what stops the two themes from drifting apart.
class SemanticAlias {
  final TonedRef light, dark;

  const SemanticAlias({required this.light, required this.dark});

  Color resolve(Brightness brightness) =>
      (brightness == Brightness.dark ? dark : light).resolve();
}

/// Typed reference to one slot in the semantic palette. Adding a case here
/// forces [SemanticColorAlias.alias] to define its light/dark pair.
enum SemanticColor {
  accentBrandPrimary,
  accentBrandSecondary,
  accentBrandTertiary,
  accentBrandQuaternary,
  backgroundBasePrimary,
  backgroundBaseSecondary,
  backgroundBaseTertiary,
  backgroundBaseQuaternary,
  backgroundBaseQuinary,
  backgroundElevatedPrimary,
  backgroundElevatedSecondary,
  backgroundElevatedTertiary,
  backgroundElevatedQuaternary,
  backgroundElevatedQuinary,
  backgroundMaterialPrimary,
  backgroundMaterialSecondary,
  backgroundMaterialTertiary,
  backgroundMaterialQuaternary,
  textPrimary,
  textSecondary,
  textTertiary,
  textQuaternary,
  separatorPrimary,
  separatorSecondary,
  fillPrimary,
  fillSecondary,
  fillTertiary,
  functionNeutralWhite,
  functionNeutralBlack,
  functionNeutralToggleWhite,
  functionNeutralToggleBlack,
  functionNeutralScrim,
  functionNeutralScrimDark,
  functionLinkPrimary,
  functionDangerPrimary,
  functionSuccessPrimary,
  functionSuccessSecondary,
  functionWarningPrimary,
  functionWarningSecondary,
}

extension SemanticColorAlias on SemanticColor {
  /// Canonical light/dark pair for this slot. The resolved bundles derive
  /// from here, so a primitive swap reaches every slot that names it.
  SemanticAlias get alias => switch (this) {
    SemanticColor.accentBrandPrimary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s1000),
      dark: NeutralRef(NeutralShade.s0),
    ),
    SemanticColor.accentBrandSecondary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s700),
      dark: NeutralRef(NeutralShade.s300),
    ),
    SemanticColor.accentBrandTertiary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s400),
      dark: NeutralRef(NeutralShade.s600),
    ),
    SemanticColor.accentBrandQuaternary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s150),
      dark: NeutralRef(NeutralShade.s850),
    ),
    SemanticColor.backgroundBasePrimary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0),
      dark: NeutralRef(NeutralShade.s1000),
    ),
    SemanticColor.backgroundBaseSecondary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s100),
      dark: NeutralRef(NeutralShade.s900),
    ),
    SemanticColor.backgroundBaseTertiary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0),
      dark: NeutralRef(NeutralShade.s1000),
    ),
    SemanticColor.backgroundBaseQuaternary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s150),
      dark: NeutralRef(NeutralShade.s850),
    ),
    SemanticColor.backgroundBaseQuinary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0),
      dark: NeutralRef(NeutralShade.s900),
    ),
    SemanticColor.backgroundElevatedPrimary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0),
      dark: NeutralRef(NeutralShade.s850),
    ),
    SemanticColor.backgroundElevatedSecondary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s100),
      dark: NeutralRef(NeutralShade.s800),
    ),
    SemanticColor.backgroundElevatedTertiary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0),
      dark: NeutralRef(NeutralShade.s700),
    ),
    SemanticColor.backgroundElevatedQuaternary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s150),
      dark: NeutralRef(NeutralShade.s600),
    ),
    SemanticColor.backgroundElevatedQuinary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s100),
      dark: NeutralRef(NeutralShade.s900),
    ),
    SemanticColor.backgroundMaterialPrimary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0, _materialFillAlpha),
      dark: NeutralRef(NeutralShade.s1000, _materialFillAlpha),
    ),
    SemanticColor.backgroundMaterialSecondary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s50, _materialFillAlpha),
      dark: NeutralRef(NeutralShade.s950, _materialFillAlpha),
    ),
    SemanticColor.backgroundMaterialTertiary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0, _materialFillAlpha),
      dark: NeutralRef(NeutralShade.s900, _materialFillAlpha),
    ),
    SemanticColor.backgroundMaterialQuaternary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s100, _materialFillAlpha),
      dark: NeutralRef(NeutralShade.s900, _materialFillAlpha),
    ),
    SemanticColor.textPrimary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.94),
      dark: NeutralRef(NeutralShade.s0, 0.94),
    ),
    SemanticColor.textSecondary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.85),
      dark: NeutralRef(NeutralShade.s0, 0.85),
    ),
    SemanticColor.textTertiary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.60),
      dark: NeutralRef(NeutralShade.s0, 0.60),
    ),
    SemanticColor.textQuaternary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.40),
      dark: NeutralRef(NeutralShade.s0, 0.40),
    ),
    SemanticColor.separatorPrimary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.20),
      dark: NeutralRef(NeutralShade.s0, 0.30),
    ),
    SemanticColor.separatorSecondary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.10),
      dark: NeutralRef(NeutralShade.s0, 0.20),
    ),
    SemanticColor.fillPrimary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.15),
      dark: NeutralRef(NeutralShade.s0, 0.20),
    ),
    SemanticColor.fillSecondary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.10),
      dark: NeutralRef(NeutralShade.s0, 0.15),
    ),
    SemanticColor.fillTertiary => const SemanticAlias(
      light: NeutralRef(NeutralShade.s900, 0.05),
      dark: NeutralRef(NeutralShade.s0, 0.10),
    ),
    // White and black stay put across modes: they are ink for a surface whose
    // color is fixed, such as avatar initials or lightbox glass. The
    // mode-following counterparts are toggleWhite and toggleBlack.
    SemanticColor.functionNeutralWhite => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0),
      dark: NeutralRef(NeutralShade.s0),
    ),
    SemanticColor.functionNeutralBlack => const SemanticAlias(
      light: NeutralRef(NeutralShade.s1000),
      dark: NeutralRef(NeutralShade.s1000),
    ),
    SemanticColor.functionNeutralToggleWhite => const SemanticAlias(
      light: NeutralRef(NeutralShade.s0),
      dark: NeutralRef(NeutralShade.s1000),
    ),
    SemanticColor.functionNeutralToggleBlack => const SemanticAlias(
      light: NeutralRef(NeutralShade.s1000),
      dark: NeutralRef(NeutralShade.s0),
    ),
    SemanticColor.functionNeutralScrim => const SemanticAlias(
      light: NeutralRef(NeutralShade.s1000, 0.20),
      dark: NeutralRef(NeutralShade.s1000, 0.80),
    ),
    SemanticColor.functionNeutralScrimDark => const SemanticAlias(
      light: NeutralRef(NeutralShade.s1000, 0.90),
      dark: NeutralRef(NeutralShade.s1000, 0.90),
    ),
    SemanticColor.functionLinkPrimary => const SemanticAlias(
      light: ChromaticRef(Hue.blue, Shade.s400),
      dark: ChromaticRef(Hue.blue, Shade.s500),
    ),
    SemanticColor.functionDangerPrimary => const SemanticAlias(
      light: ChromaticRef(Hue.red, Shade.s400),
      dark: ChromaticRef(Hue.red, Shade.s500),
    ),
    SemanticColor.functionSuccessPrimary => const SemanticAlias(
      light: ChromaticRef(Hue.green, Shade.s400),
      dark: ChromaticRef(Hue.green, Shade.s500),
    ),
    SemanticColor.functionSuccessSecondary => const SemanticAlias(
      light: ChromaticRef(Hue.green, Shade.s100),
      dark: ChromaticRef(Hue.green, Shade.s900),
    ),
    SemanticColor.functionWarningPrimary => const SemanticAlias(
      light: ChromaticRef(Hue.yellow, Shade.s400),
      dark: ChromaticRef(Hue.yellow, Shade.s500),
    ),
    SemanticColor.functionWarningSecondary => const SemanticAlias(
      light: ChromaticRef(Hue.yellow, Shade.s100),
      dark: ChromaticRef(Hue.yellow, Shade.s900),
    ),
  };

  Color resolve(Brightness brightness) => alias.resolve(brightness);
}

class SemanticPalette {
  final AccentBrand accentBrand;
  final BackgroundBase backgroundBase;
  final BackgroundElevated backgroundElevated;
  final BackgroundMaterial backgroundMaterial;
  final TextPalette text;
  final SeparatorPalette separator;
  final FillPalette fill;
  final FunctionPalette function;
  final MessagePalette message;

  const SemanticPalette({
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

  factory SemanticPalette.from(Brightness brightness) {
    Color r(SemanticColor slot) => slot.resolve(brightness);
    return SemanticPalette(
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
      text: TextPalette(
        primary: r(SemanticColor.textPrimary),
        secondary: r(SemanticColor.textSecondary),
        tertiary: r(SemanticColor.textTertiary),
        quaternary: r(SemanticColor.textQuaternary),
      ),
      separator: SeparatorPalette(
        primary: r(SemanticColor.separatorPrimary),
        secondary: r(SemanticColor.separatorSecondary),
      ),
      fill: FillPalette(
        primary: r(SemanticColor.fillPrimary),
        secondary: r(SemanticColor.fillSecondary),
        tertiary: r(SemanticColor.fillTertiary),
      ),
      function: FunctionPalette(
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
          ? _darkMessagePalette
          : _lightMessagePalette,
    );
  }

  static SemanticPalette of(BuildContext context) {
    return MediaQuery.platformBrightnessOf(context) == Brightness.dark
        ? darkSemanticPalette
        : lightSemanticPalette;
  }
}

final SemanticPalette lightSemanticPalette = SemanticPalette.from(
  Brightness.light,
);

final SemanticPalette darkSemanticPalette = SemanticPalette.from(
  Brightness.dark,
);

/// Message-bubble colors are the one bundle without aliases: they are an
/// Air-specific extension that the reference DS carries in its message-bubble
/// pattern tokens rather than in the semantic palette.
final MessagePalette _lightMessagePalette = MessagePalette(
  selfBackground: Primitive.neutral(NeutralShade.s150),
  otherBackground: Primitive.neutral(NeutralShade.s100),
  selfText: Primitive.neutral(NeutralShade.s1000),
  otherText: Primitive.neutral(NeutralShade.s1000),
  selfListPrefix: Primitive.neutral(NeutralShade.s800),
  otherListPrefix: Primitive.neutral(NeutralShade.s800),
  selfQuoteBorder: Primitive.chromatic(Hue.blue, Shade.s500),
  otherQuoteBorder: Primitive.chromatic(Hue.blue, Shade.s500),
  selfQuoteBackground: Primitive.chromatic(Hue.blue, Shade.s50),
  otherQuoteBackground: Primitive.chromatic(Hue.blue, Shade.s50),
  selfTableBorder: Primitive.neutral(NeutralShade.s300),
  otherTableBorder: Primitive.neutral(NeutralShade.s300),
  selfCheckboxBorder: Primitive.neutral(NeutralShade.s400),
  otherCheckboxBorder: Primitive.neutral(NeutralShade.s400),
  selfCheckboxFill: Primitive.neutral(NeutralShade.s200),
  otherCheckboxFill: Primitive.neutral(NeutralShade.s200),
  selfCheckboxCheck: Primitive.neutral(NeutralShade.s1000),
  otherCheckboxCheck: Primitive.neutral(NeutralShade.s1000),
  selfEditedLabel: Primitive.neutral(NeutralShade.s600),
  otherEditedLabel: Primitive.neutral(NeutralShade.s600),
);

final MessagePalette _darkMessagePalette = MessagePalette(
  selfBackground: Primitive.neutral(NeutralShade.s850),
  otherBackground: Primitive.neutral(NeutralShade.s900),
  selfText: Primitive.neutral(NeutralShade.s0),
  otherText: Primitive.neutral(NeutralShade.s0),
  selfListPrefix: Primitive.neutral(NeutralShade.s200),
  otherListPrefix: Primitive.neutral(NeutralShade.s200),
  selfQuoteBorder: Primitive.chromatic(Hue.blue, Shade.s600),
  otherQuoteBorder: Primitive.chromatic(Hue.blue, Shade.s600),
  selfQuoteBackground: Primitive.chromatic(Hue.blue, Shade.s800),
  otherQuoteBackground: Primitive.chromatic(Hue.blue, Shade.s800),
  selfTableBorder: Primitive.neutral(NeutralShade.s800),
  otherTableBorder: Primitive.neutral(NeutralShade.s800),
  selfCheckboxBorder: Primitive.neutral(NeutralShade.s600),
  otherCheckboxBorder: Primitive.neutral(NeutralShade.s600),
  selfCheckboxFill: Primitive.neutral(NeutralShade.s700),
  otherCheckboxFill: Primitive.neutral(NeutralShade.s700),
  selfCheckboxCheck: Primitive.neutral(NeutralShade.s0),
  otherCheckboxCheck: Primitive.neutral(NeutralShade.s0),
  selfEditedLabel: Primitive.neutral(NeutralShade.s400),
  otherEditedLabel: Primitive.neutral(NeutralShade.s400),
);
