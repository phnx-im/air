// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ds/foundations/color_theme.dart';
import 'package:air/ds/foundations/primitives.dart';
import 'package:air/ds/foundations/role_palettes.dart';
import 'package:air/ds/foundations/roles.dart';

/// The accent roles as fills with their ink, plus [quaternary], a container
/// tinted by the primary. [hover] is the fill a pointer hover or a lit row
/// takes, [hoverTint] the same hover as a translucent wash for a surface whose
/// content keeps its own colors.
class AccentBrand {
  final Color primary, secondary, tertiary, quaternary;
  final Color onPrimary, onSecondary, onTertiary;
  final Color hover, onHover, hoverTint;

  const AccentBrand({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
    required this.onPrimary,
    required this.onSecondary,
    required this.onTertiary,
    required this.hover,
    required this.onHover,
    required this.hoverTint,
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

  TextPalette({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });

  final Map<int, TextPalette> _opaque = {};

  /// All four slots composited onto [background], each fully opaque.
  TextPalette on(Color background) {
    assert(background.a == 1.0, 'background must be opaque: $background');
    assert(
      _opaque.length < 16,
      'text palette cache growing too much: are you pushing animated values?',
    );
    // Keyed on the packed value, which is unique here: the assert above fixes
    // the alpha byte at 0xFF.
    return _opaque[background.toARGB32()] ??= TextPalette(
      primary: primary.on(background),
      secondary: secondary.on(background),
      tertiary: tertiary.on(background),
      quaternary: quaternary.on(background),
    );
  }
}

extension OpaqueOn on Color {
  /// This color composited onto [background], fully opaque.
  Color on(Color background) {
    assert(background.a == 1.0, 'background must be opaque: $background');
    return Color.alphaBlend(this, background);
  }
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
  final Color selfTableBorder, otherTableBorder;
  final Color selfCheckboxCheck, otherCheckboxCheck;

  const MessagePalette({
    required this.selfBackground,
    required this.otherBackground,
    required this.selfText,
    required this.otherText,
    required this.selfListPrefix,
    required this.otherListPrefix,
    required this.selfTableBorder,
    required this.otherTableBorder,
    required this.selfCheckboxCheck,
    required this.otherCheckboxCheck,
  });
}

/// Alpha applied to every material fill, so a blurred surface still masks the
/// content moving behind it.
const double _materialFillAlpha = 0.5;

/// Alpha of an accent over a surface where it acts as a container tint.
/// Noctalia's cards and pills use 0.15 to 0.18.
const double _containerTintAlpha = 0.18;

/// Alpha of an opaque hover role used as a wash.
const double _hoverWashAlpha = 0.4;

/// Alpha of an accent over the surface for a message bubble. Higher than a
/// card tint, so the two sides separate from each other and from the surface.
const double _bubbleTintAlpha = 0.35;

/// The semantic slots the app paints with, derived from a [RolePalette]. The
/// roles are the design, the slots are how the code has been reading them.
class SemanticPalette {
  /// The roles this palette derives from. Components that follow Noctalia's
  /// element to role map bind here directly.
  final RolePalette roles;

  /// The ramps of the theme, for components that pick their own shades such
  /// as avatar gradients.
  final PrimitivePalette primitives;
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
    required this.roles,
    required this.primitives,
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

  factory SemanticPalette.from(
    RolePalette r, [
    PrimitivePalette primitives = flexokiPrimitives,
  ]) {
    final dark = r.isDark;
    const white = Color(0xFFFFFFFF);
    const black = Color(0xFF000000);
    Color onSurfaceAt(double alpha) => r.onSurface.withValues(alpha: alpha);
    Color material(Color surface) =>
        surface.withValues(alpha: _materialFillAlpha);
    // A translucent hover is already a wash. An opaque one is thinned for
    // surfaces that keep their own ink.
    final hoverTint = r.hover.a < 1.0
        ? r.hover
        : r.hover.withValues(alpha: _hoverWashAlpha);

    return SemanticPalette(
      roles: r,
      primitives: primitives,
      accentBrand: AccentBrand(
        primary: r.primary,
        onPrimary: r.onPrimary,
        secondary: r.secondary,
        onSecondary: r.onSecondary,
        tertiary: r.tertiary,
        onTertiary: r.onTertiary,
        quaternary: r.primary
            .withValues(alpha: _containerTintAlpha)
            .on(r.surface),
        hover: r.hover,
        onHover: r.onHover,
        hoverTint: hoverTint,
      ),
      backgroundBase: BackgroundBase(
        primary: r.surface,
        secondary: r.surfaceContainerLow,
        tertiary: r.surface,
        quaternary: r.surfaceVariant,
        quinary: r.surfaceContainerLow,
      ),
      backgroundElevated: BackgroundElevated(
        primary: r.surfaceVariant,
        secondary: r.surfaceContainerHigh,
        tertiary: r.surfaceContainerHighest,
        quaternary: r.surfaceBright,
        quinary: r.surfaceContainerLow,
      ),
      backgroundMaterial: BackgroundMaterial(
        primary: material(r.surface),
        secondary: material(r.surfaceContainerLowest),
        tertiary: material(r.surfaceContainerLow),
        quaternary: material(r.surfaceContainerLow),
      ),
      text: TextPalette(
        primary: r.onSurface,
        secondary: onSurfaceAt(0.85),
        tertiary: r.onSurfaceVariant,
        quaternary: r.onSurfaceVariant.withValues(alpha: 0.6),
      ),
      separator: SeparatorPalette(
        primary: r.outline,
        secondary: r.outlineVariant,
      ),
      fill: FillPalette(
        primary: onSurfaceAt(0.20),
        secondary: onSurfaceAt(0.15),
        tertiary: onSurfaceAt(0.10),
      ),
      function: FunctionPalette(
        neutral: FunctionNeutral(
          white: white,
          black: black,
          toggleWhite: dark ? black : white,
          toggleBlack: dark ? white : black,
          scrim: black.withValues(alpha: dark ? 0.80 : 0.20),
          scrimDark: black.withValues(alpha: 0.90),
        ),
        link: r.primary,
        danger: r.error,
        // Noctalia has no success or warning role. These come from the
        // theme's green and yellow ramps.
        success: FunctionSuccess(
          primary: primitives.chromatic(
            Hue.green,
            dark ? Shade.s500 : Shade.s400,
          ),
          secondary: primitives.chromatic(
            Hue.green,
            dark ? Shade.s900 : Shade.s100,
          ),
        ),
        warning: FunctionWarning(
          primary: primitives.chromatic(
            Hue.yellow,
            dark ? Shade.s500 : Shade.s400,
          ),
          secondary: primitives.chromatic(
            Hue.yellow,
            dark ? Shade.s900 : Shade.s100,
          ),
        ),
      ),
      // Noctalia has no message roles. Both sides take an accent as a tint
      // over the surface: yours the primary, the other side's the secondary.
      message: MessagePalette(
        selfBackground: r.primary
            .withValues(alpha: _bubbleTintAlpha)
            .on(r.surface),
        otherBackground: r.secondary
            .withValues(alpha: _bubbleTintAlpha)
            .on(r.surface),
        selfText: r.onSurface,
        otherText: r.onSurface,
        selfListPrefix: r.onSurfaceVariant,
        otherListPrefix: r.onSurfaceVariant,
        selfTableBorder: r.outline,
        otherTableBorder: r.outline,
        selfCheckboxCheck: r.onSurface,
        otherCheckboxCheck: r.onSurface,
      ),
    );
  }

  /// The palette for the ambient theme. Under a `MaterialApp` this follows
  /// the active color theme and brightness, elsewhere the default theme and
  /// the platform brightness.
  static SemanticPalette of(BuildContext context) {
    final theme = Theme.of(context);
    final palettes = theme.extension<ThemePalettes>();
    if (palettes == null) {
      return MediaQuery.platformBrightnessOf(context) == .dark
          ? darkSemanticPalette
          : lightSemanticPalette;
    }
    return theme.brightness == .dark ? palettes.dark : palettes.light;
  }

  /// The active color theme's dark palette regardless of brightness, for
  /// surfaces that are always dark such as the image viewer.
  static SemanticPalette darkOf(BuildContext context) =>
      Theme.of(context).extension<ThemePalettes>()?.dark ?? darkSemanticPalette;
}

/// Both palettes of one color theme, carried on `ThemeData` so that
/// [SemanticPalette.of] follows the user's theme choice.
class ThemePalettes extends ThemeExtension<ThemePalettes> {
  final SemanticPalette light, dark;

  const ThemePalettes({required this.light, required this.dark});

  ThemePalettes.from(ColorTheme theme)
    : light = SemanticPalette.from(theme.light, theme.primitives),
      dark = SemanticPalette.from(theme.dark, theme.primitives);

  @override
  ThemePalettes copyWith({SemanticPalette? light, SemanticPalette? dark}) =>
      ThemePalettes(light: light ?? this.light, dark: dark ?? this.dark);

  /// Palettes are not interpolated, a theme switch snaps halfway through the
  /// theme animation.
  @override
  ThemePalettes lerp(ThemePalettes? other, double t) =>
      other == null || t < 0.5 ? this : other;
}

final SemanticPalette lightSemanticPalette = SemanticPalette.from(airLight);

final SemanticPalette darkSemanticPalette = SemanticPalette.from(airDark);
