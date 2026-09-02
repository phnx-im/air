// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/painting.dart';

/// A color theme as sixteen roles, the way Noctalia defines a palette. Every
/// element binds to one role, everything else is derived from them here,
/// mirroring `fixed_palette.cpp` in the Noctalia sources.
///
/// [primary] is the accent for active and checked states, selection, links
/// and calls to action. [secondary] is the accent for headings and badges.
/// [tertiary] is a third accent, rarely used as ink. [hover] is the fill a
/// pointer hover or a lit row takes. Each fill has its ink as `on*`.
///
/// [selection] and [navSelection] are the roles Noctalia does not have: the
/// fill of the selected list row, and of the selected navigation cell (rail,
/// tab bar, you menu). Noctalia paints both in [primary], and so does every
/// palette copied from it. A neutral theme needs them apart from [primary],
/// since a grey that reads as selection makes a toggle unreadable, and Air's
/// design lifts a selected row but recesses a selected tab.
class RolePalette {
  const RolePalette({
    required this.primary,
    required this.onPrimary,
    required this.secondary,
    required this.onSecondary,
    required this.tertiary,
    required this.onTertiary,
    required this.error,
    required this.onError,
    required this.surface,
    required this.onSurface,
    required this.surfaceVariant,
    required this.onSurfaceVariant,
    required this.outline,
    required this.shadow,
    required this.hover,
    required this.onHover,
    this._selection,
    this._onSelection,
    this._navSelection,
    this._onNavSelection,
    this.bubbleSelf,
    this.bubbleOther,
  });

  final Color primary, onPrimary;
  final Color secondary, onSecondary;
  final Color tertiary, onTertiary;
  final Color error, onError;
  final Color surface, onSurface;
  final Color surfaceVariant, onSurfaceVariant;
  final Color outline, shadow;

  /// May be translucent. A translucent hover is a wash over whatever it sits
  /// on, an opaque one is a fill in its own right.
  final Color hover, onHover;

  final Color? _selection, _onSelection, _navSelection, _onNavSelection;

  /// Fill of a selected list row. [primary] unless the theme says otherwise.
  /// Like [hover] it may be translucent, a wash over the surface it sits on.
  Color get selection => _selection ?? primary;
  Color get onSelection => _onSelection ?? onPrimary;

  /// Fill of the selected navigation cell. [selection] unless the theme says
  /// otherwise.
  Color get navSelection => _navSelection ?? selection;
  Color get onNavSelection => _onNavSelection ?? onSelection;

  /// Message bubble fills, yours and the other side's. Null derives them as
  /// tints of [primary] and [secondary] over [surface]. A theme pins them
  /// where its bubbles are not accent-tinted, like Air's neutral greys.
  final Color? bubbleSelf, bubbleOther;

  bool get isDark => surface.computeLuminance() < 0.5;

  // Surface tiers between [surface] and [surfaceVariant] and above it.

  Color get surfaceContainerLowest => Color.lerp(surface, surfaceVariant, 0.2)!;
  Color get surfaceContainerLow => Color.lerp(surface, surfaceVariant, 0.5)!;
  Color get surfaceContainer => surfaceVariant;
  Color get surfaceContainerHigh => isDark
      ? _lightness(surfaceVariant, 0.04, max: 0.40)
      : _lightness(surfaceVariant, -0.04, min: 0.60);
  Color get surfaceContainerHighest => isDark
      ? _lightness(surfaceVariant, 0.08, max: 0.45)
      : _lightness(surfaceVariant, -0.08, min: 0.55);
  Color get surfaceBright => isDark
      ? _lightness(surfaceVariant, 0.12, max: 0.50)
      : _lightness(surface, 0.03, max: 0.98);
  Color get surfaceDim => isDark
      ? _lightness(surface, -0.04, min: 0.02)
      : _lightness(surfaceVariant, -0.12, min: 0.50);

  Color get outlineVariant => isDark
      ? _lightness(outline, -0.15, min: 0.10)
      : _lightness(outline, 0.15, max: 0.90);

  // Containers: a muted, low-contrast version of an accent, for a surface
  // tinted by it.

  Color get primaryContainer => _container(primary);
  Color get secondaryContainer => _container(secondary);
  Color get tertiaryContainer => _container(tertiary);
  Color get errorContainer => _container(error);

  Color _container(Color base) {
    final hsl = HSLColor.fromColor(base);
    final s = hsl.saturation;
    final l = hsl.lightness;
    return isDark
        ? hsl
              .withSaturation((s + 0.15 * (1.0 - s)).clamp(0.0, 1.0))
              .withLightness((l - 0.35).clamp(0.15, 1.0))
              .toColor()
        : hsl
              .withSaturation((s - 0.20).clamp(0.30, 1.0))
              .withLightness((l + 0.35).clamp(0.0, 0.85))
              .toColor();
  }

  static Color _lightness(
    Color color,
    double delta, {
    double min = 0.0,
    double max = 1.0,
  }) {
    final hsl = HSLColor.fromColor(color);
    return hsl.withLightness((hsl.lightness + delta).clamp(min, max)).toColor();
  }
}
