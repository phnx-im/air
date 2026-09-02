// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:ui';

import 'package:air/ds/foundations/oklch.dart';
import 'package:air/ds/foundations/primitives.dart';
import 'package:air/ds/foundations/role_palettes.dart';
import 'package:air/ds/foundations/roles.dart';

/// A named theme: one [RolePalette] per mode, which is the design, plus
/// generated chromatic ramps for the few components that pick their own
/// shades (avatar gradients). The ramps use [flexokiPrimitives] as the
/// perceptual template so every theme keeps the same spacing as the default.
class ColorTheme {
  const ColorTheme({
    required this.id,
    required this.name,
    required this.dark,
    required this.light,
    required this.lightBackground,
    required this.darkBackground,
    required this.accents,
    required this.swatches,
    this._primitives,
  });

  /// The roles per mode.
  final RolePalette dark, light;

  /// Stable snake_case identifier, e.g. `'gruvbox'`.
  final String id;

  /// Display name, e.g. `'Gruvbox'`.
  final String name;

  /// Becomes [NeutralShade.s0].
  final Color lightBackground;

  /// Becomes [NeutralShade.s1000].
  final Color darkBackground;

  /// One color per [Hue], anchored at [Shade.s400].
  final Map<Hue, Color> accents;

  /// Exactly 4 hues shown in the theme picker preview.
  final List<Hue> swatches;

  final PrimitivePalette? _primitives;

  static final Map<String, PrimitivePalette> _cache = {};

  /// Generated once per [id] on first access (the class is const, so the
  /// cache lives outside the instance).
  PrimitivePalette get primitives {
    final given = _primitives;
    if (given != null) return given;
    return _cache.putIfAbsent(id, _generate);
  }

  PrimitivePalette _generate() {
    final lightLab = lightBackground.toOklab();
    final darkLab = darkBackground.toOklab();

    final neutral = <NeutralShade, Color>{};
    for (final shade in NeutralShade.values) {
      final t = _neutralPosition[shade]!;
      neutral[shade] = lerpOklab(lightLab, darkLab, t).toColor();
    }

    final chromatic = <(Hue, Shade), Color>{};
    for (final hue in Hue.values) {
      final accent = accents[hue];
      if (accent == null) {
        throw StateError('ColorTheme $id is missing an accent for ${hue.name}');
      }
      final a = accent.toOklch();
      final t400 = flexokiPrimitives.chromatic(hue, Shade.s400).toOklch();
      for (final shade in Shade.values) {
        final ts = flexokiPrimitives.chromatic(hue, shade).toOklch();
        final l = (a.l + (ts.l - t400.l)).clamp(0.0, 1.0);
        final c = t400.c == 0 ? 0.0 : a.c * (ts.c / t400.c);
        chromatic[(hue, shade)] = Oklch(l, c, a.h).toColor();
      }
    }

    return PrimitivePalette(neutral: neutral, chromatic: chromatic);
  }
}

/// Where each neutral shade sits between the light background (0) and the
/// dark background (1). The dark end is compressed: the dark surfaces the app
/// paints with s1000 down to s800 stay close to the theme's own background,
/// the way Gruvbox steps bg0, bg1, bg2. The default palette does not use this
/// table, its ramp is the Flexoki one verbatim.
const Map<NeutralShade, double> _neutralPosition = {
  NeutralShade.s0: 0.0,
  NeutralShade.s50: 0.02,
  NeutralShade.s100: 0.05,
  NeutralShade.s150: 0.10,
  NeutralShade.s200: 0.17,
  NeutralShade.s300: 0.25,
  NeutralShade.s400: 0.34,
  NeutralShade.s500: 0.44,
  NeutralShade.s600: 0.56,
  NeutralShade.s700: 0.68,
  NeutralShade.s800: 0.80,
  NeutralShade.s850: 0.88,
  NeutralShade.s900: 0.95,
  NeutralShade.s950: 0.98,
  NeutralShade.s1000: 1.0,
};

/// The theme the app starts in.
ColorTheme get defaultColorTheme => builtinColorThemes.first;

/// All built-in themes, the default first.
const List<ColorTheme> builtinColorThemes = [
  ColorTheme(
    id: 'air',
    dark: airDark,
    light: airLight,
    name: 'Air',
    primitives: flexokiPrimitives,
    lightBackground: Color(0xFFFFFFFF),
    darkBackground: Color(0xFF000000),
    accents: {
      Hue.red: Color(0xFFD14D41),
      Hue.orange: Color(0xFFDA702C),
      Hue.yellow: Color(0xFFD0A215),
      Hue.green: Color(0xFF879A39),
      Hue.cyan: Color(0xFF3AA99F),
      Hue.blue: Color(0xFF4385BE),
      Hue.purple: Color(0xFF8B7EC8),
      Hue.magenta: Color(0xFFCE5D97),
    },
    swatches: [Hue.red, Hue.yellow, Hue.green, Hue.blue],
  ),

  // Ayu dark terminal colors (noctalia's builtin_palettes.cpp).
  ColorTheme(
    id: 'ayu',
    dark: ayuDark,
    light: ayuLight,
    name: 'Ayu',
    lightBackground: Color(0xFFF8F9FA),
    darkBackground: Color(0xFF0B0E14),
    accents: {
      Hue.red: Color(0xFFF07178),
      Hue.orange: Color(0xFFFF8F40),
      Hue.yellow: Color(0xFFE6B450),
      Hue.green: Color(0xFFAAD94C),
      Hue.cyan: Color(0xFF95E6CB),
      Hue.blue: Color(0xFF39BAE6),
      Hue.purple: Color(0xFFD2A6FF),
      Hue.magenta: Color(0xFFF07178),
    },
    swatches: [Hue.yellow, Hue.orange, Hue.cyan, Hue.red],
  ),

  // Catppuccin Mocha accents.
  ColorTheme(
    id: 'catppuccin',
    dark: catppuccinDark,
    light: catppuccinLight,
    name: 'Catppuccin',
    lightBackground: Color(0xFFEFF1F5),
    darkBackground: Color(0xFF1E1E2E),
    accents: {
      Hue.red: Color(0xFFF38BA8),
      Hue.orange: Color(0xFFFAB387),
      Hue.yellow: Color(0xFFF9E2AF),
      Hue.green: Color(0xFFA6E3A1),
      Hue.cyan: Color(0xFF94E2D5),
      Hue.blue: Color(0xFF89B4FA),
      Hue.purple: Color(0xFFCBA6F7),
      Hue.magenta: Color(0xFFF5C2E7),
    },
    swatches: [Hue.purple, Hue.magenta, Hue.cyan, Hue.red],
  ),

  // Dracula palette.
  ColorTheme(
    id: 'dracula',
    dark: draculaDark,
    light: draculaLight,
    name: 'Dracula',
    lightBackground: Color(0xFFF8F8F2),
    darkBackground: Color(0xFF282A36),
    accents: {
      Hue.red: Color(0xFFFF5555),
      Hue.orange: Color(0xFFFFB86C),
      Hue.yellow: Color(0xFFF1FA8C),
      Hue.green: Color(0xFF50FA7B),
      Hue.cyan: Color(0xFF8BE9FD),
      Hue.blue: Color(0xFF6272A4),
      Hue.purple: Color(0xFFBD93F9),
      Hue.magenta: Color(0xFFFF79C6),
    },
    swatches: [Hue.purple, Hue.magenta, Hue.green, Hue.red],
  ),

  // Eldritch dark terminal colors (noctalia's builtin_palettes.cpp). Orange
  // has no counterpart in the source palette; it is interpolated between
  // Eldritch's own red and yellow. Purple uses the bright "blue" ANSI slot,
  // which is actually violet in this palette.
  ColorTheme(
    id: 'eldritch',
    dark: eldritchDark,
    light: eldritchLight,
    name: 'Eldritch',
    lightBackground: Color(0xFFF0F3F4),
    darkBackground: Color(0xFF212337),
    accents: {
      Hue.red: Color(0xFFF9515D),
      Hue.orange: Color(0xFFF9915A),
      Hue.yellow: Color(0xFFE9F941),
      Hue.green: Color(0xFF37F499),
      Hue.cyan: Color(0xFF04D1F9),
      Hue.blue: Color(0xFF9071F4),
      Hue.purple: Color(0xFFA48CF2),
      Hue.magenta: Color(0xFFF265B5),
    },
    swatches: [Hue.green, Hue.cyan, Hue.purple, Hue.red],
  ),

  // Gruvbox (bright variant, matching the upstream terminal palette).
  ColorTheme(
    id: 'gruvbox',
    dark: gruvboxDark,
    light: gruvboxLight,
    name: 'Gruvbox',
    lightBackground: Color(0xFFFBF1C7),
    darkBackground: Color(0xFF282828),
    accents: {
      Hue.red: Color(0xFFFB4934),
      Hue.orange: Color(0xFFFE8019),
      Hue.yellow: Color(0xFFFABD2F),
      Hue.green: Color(0xFFB8BB26),
      Hue.cyan: Color(0xFF8EC07C),
      Hue.blue: Color(0xFF83A598),
      Hue.purple: Color(0xFFB16286),
      Hue.magenta: Color(0xFFD3869B),
    },
    swatches: [Hue.yellow, Hue.orange, Hue.cyan, Hue.red],
  ),

  // Kanagawa (bright variant).
  ColorTheme(
    id: 'kanagawa',
    dark: kanagawaDark,
    light: kanagawaLight,
    name: 'Kanagawa',
    lightBackground: Color(0xFFF2ECBC),
    darkBackground: Color(0xFF1F1F28),
    accents: {
      Hue.red: Color(0xFFE46876),
      Hue.orange: Color(0xFFFFA066),
      Hue.yellow: Color(0xFFE6C384),
      Hue.green: Color(0xFF98BB6C),
      Hue.cyan: Color(0xFF7AA89F),
      Hue.blue: Color(0xFF7E9CD8),
      Hue.purple: Color(0xFF957FB8),
      Hue.magenta: Color(0xFFD27E99),
    },
    swatches: [Hue.green, Hue.blue, Hue.purple, Hue.red],
  ),

  // Noctalia dark terminal colors. The source palette itself doubles
  // magenta on red and cyan on green; orange has no source value and is
  // interpolated between Noctalia's red and yellow, and purple reuses
  // magenta per the same doubling.
  ColorTheme(
    id: 'noctalia',
    dark: noctaliaDark,
    light: noctaliaLight,
    name: 'Noctalia',
    lightBackground: Color(0xFFE6E8FA),
    darkBackground: Color(0xFF070722),
    accents: {
      Hue.red: Color(0xFFFD4663),
      Hue.orange: Color(0xFFFD9563),
      Hue.yellow: Color(0xFFFFF59B),
      Hue.green: Color(0xFF9BFECE),
      Hue.cyan: Color(0xFF9BFECE),
      Hue.blue: Color(0xFFA9AEFE),
      Hue.purple: Color(0xFFFD4663),
      Hue.magenta: Color(0xFFFD4663),
    },
    swatches: [Hue.yellow, Hue.blue, Hue.green, Hue.red],
  ),

  // Nord (aurora/frost accents, matching the upstream terminal palette).
  ColorTheme(
    id: 'nord',
    dark: nordDark,
    light: nordLight,
    name: 'Nord',
    lightBackground: Color(0xFFECEFF4),
    darkBackground: Color(0xFF2E3440),
    accents: {
      Hue.red: Color(0xFFBF616A),
      Hue.orange: Color(0xFFD08770),
      Hue.yellow: Color(0xFFEBCB8B),
      Hue.green: Color(0xFFA3BE8C),
      Hue.cyan: Color(0xFF88C0D0),
      Hue.blue: Color(0xFF81A1C1),
      Hue.purple: Color(0xFFB48EAD),
      Hue.magenta: Color(0xFFB48EAD),
    },
    swatches: [Hue.cyan, Hue.blue, Hue.purple, Hue.red],
  ),

  // Rose Pine.
  ColorTheme(
    id: 'rose_pine',
    dark: rosePineDark,
    light: rosePineLight,
    name: 'Rosé Pine',
    lightBackground: Color(0xFFFFFAF3),
    darkBackground: Color(0xFF191724),
    accents: {
      Hue.red: Color(0xFFEB6F92),
      Hue.orange: Color(0xFFF6C177),
      Hue.yellow: Color(0xFFF6C177),
      Hue.green: Color(0xFF31748F),
      Hue.cyan: Color(0xFF9CCFD8),
      Hue.blue: Color(0xFF31748F),
      Hue.purple: Color(0xFFC4A7E7),
      Hue.magenta: Color(0xFFEBBCBA),
    },
    swatches: [Hue.purple, Hue.magenta, Hue.cyan, Hue.red],
  ),

  // Tokyo Night.
  ColorTheme(
    id: 'tokyo_night',
    dark: tokyoNightDark,
    light: tokyoNightLight,
    name: 'Tokyo Night',
    lightBackground: Color(0xFFE1E2E7),
    darkBackground: Color(0xFF1A1B26),
    accents: {
      Hue.red: Color(0xFFF7768E),
      Hue.orange: Color(0xFFFF9E64),
      Hue.yellow: Color(0xFFE0AF68),
      Hue.green: Color(0xFF9ECE6A),
      Hue.cyan: Color(0xFF7DCFFF),
      Hue.blue: Color(0xFF7AA2F7),
      Hue.purple: Color(0xFFBB9AF7),
      Hue.magenta: Color(0xFFBB9AF7),
    },
    swatches: [Hue.blue, Hue.purple, Hue.cyan, Hue.red],
  ),
];

/// Looks up a theme by [ColorTheme.id], falling back to the default theme.
ColorTheme colorThemeById(String id) {
  for (final theme in builtinColorThemes) {
    if (theme.id == id) return theme;
  }
  return builtinColorThemes.first;
}
