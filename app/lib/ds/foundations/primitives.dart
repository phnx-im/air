// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/painting.dart';

/// Chromatic hues.
enum Hue { red, orange, yellow, green, cyan, blue, purple, magenta }

/// Shades that every chromatic [Hue] defines.
enum Shade {
  s50,
  s100,
  s150,
  s200,
  s300,
  s400,
  s500,
  s600,
  s700,
  s800,
  s850,
  s900,
  s950,
}

/// Neutral's shades: every [Shade] step plus the pure endpoints.
enum NeutralShade {
  s0,
  s50,
  s100,
  s150,
  s200,
  s300,
  s400,
  s500,
  s600,
  s700,
  s800,
  s850,
  s900,
  s950,
  s1000,
}

/// One complete set of primitive ramps. A color theme is a different instance
/// of this, the semantic layer stays the same.
class PrimitivePalette {
  const PrimitivePalette({required this._neutral, required this._chromatic});

  final Map<NeutralShade, Color> _neutral;
  final Map<(Hue, Shade), Color> _chromatic;

  Color neutral(NeutralShade shade) {
    final color = _neutral[shade];
    if (color == null) {
      throw StateError('Neutral palette is missing ${shade.name}');
    }
    return color;
  }

  Color chromatic(Hue hue, Shade shade) {
    final color = _chromatic[(hue, shade)];
    if (color == null) {
      throw StateError('Palette is missing ${hue.name}/${shade.name}');
    }
    return color;
  }
}

/// The default palette, the Flexoki values.
const PrimitivePalette flexokiPrimitives = PrimitivePalette(
  neutral: _neutral,
  chromatic: _chromatic,
);

/// The default primitive palette. Theme-aware code resolves through
/// `SemanticPalette` instead.
abstract final class Primitive {
  static Color neutral(NeutralShade shade) => flexokiPrimitives.neutral(shade);

  static Color chromatic(Hue hue, Shade shade) =>
      flexokiPrimitives.chromatic(hue, shade);
}

const Map<NeutralShade, Color> _neutral = {
  NeutralShade.s0: Color(0xFFFFFFFF),
  NeutralShade.s50: Color(0xFFFAFAFA),
  NeutralShade.s100: Color(0xFFF3F3F3),
  NeutralShade.s150: Color(0xFFE7E7E6),
  NeutralShade.s200: Color(0xFFDADADA),
  NeutralShade.s300: Color(0xFFCECECD),
  NeutralShade.s400: Color(0xFFC1C1C0),
  NeutralShade.s500: Color(0xFFB0B0AE),
  NeutralShade.s600: Color(0xFF949391),
  NeutralShade.s700: Color(0xFF777674),
  NeutralShade.s800: Color(0xFF5A5857),
  NeutralShade.s850: Color(0xFF3C3B3B),
  NeutralShade.s900: Color(0xFF1F1E1E),
  NeutralShade.s950: Color(0xFF0D0D0D),
  NeutralShade.s1000: Color(0xFF000000),
};

const Map<(Hue, Shade), Color> _chromatic = {
  (Hue.red, Shade.s50): Color(0xFFFFE1D5),
  (Hue.red, Shade.s100): Color(0xFFFFCABB),
  (Hue.red, Shade.s150): Color(0xFFFDB2A2),
  (Hue.red, Shade.s200): Color(0xFFF89A8A),
  (Hue.red, Shade.s300): Color(0xFFE8705F),
  (Hue.red, Shade.s400): Color(0xFFD14D41),
  (Hue.red, Shade.s500): Color(0xFFC03E35),
  (Hue.red, Shade.s600): Color(0xFFAF3029),
  (Hue.red, Shade.s700): Color(0xFF942822),
  (Hue.red, Shade.s800): Color(0xFF6C201C),
  (Hue.red, Shade.s850): Color(0xFF551B18),
  (Hue.red, Shade.s900): Color(0xFF3E1715),
  (Hue.red, Shade.s950): Color(0xFF261312),

  (Hue.orange, Shade.s50): Color(0xFFFFE7CE),
  (Hue.orange, Shade.s100): Color(0xFFFED3AF),
  (Hue.orange, Shade.s150): Color(0xFFFCC192),
  (Hue.orange, Shade.s200): Color(0xFFF9AE77),
  (Hue.orange, Shade.s300): Color(0xFFEC8B49),
  (Hue.orange, Shade.s400): Color(0xFFDA702C),
  (Hue.orange, Shade.s500): Color(0xFFCB6120),
  (Hue.orange, Shade.s600): Color(0xFFBC5215),
  (Hue.orange, Shade.s700): Color(0xFF9D4310),
  (Hue.orange, Shade.s800): Color(0xFF71320D),
  (Hue.orange, Shade.s850): Color(0xFF59290D),
  (Hue.orange, Shade.s900): Color(0xFF40200D),
  (Hue.orange, Shade.s950): Color(0xFF27180E),

  (Hue.yellow, Shade.s50): Color(0xFFFAEEC6),
  (Hue.yellow, Shade.s100): Color(0xFFF6E2A0),
  (Hue.yellow, Shade.s150): Color(0xFFF1D67E),
  (Hue.yellow, Shade.s200): Color(0xFFECCB60),
  (Hue.yellow, Shade.s300): Color(0xFFDFB431),
  (Hue.yellow, Shade.s400): Color(0xFFD0A215),
  (Hue.yellow, Shade.s500): Color(0xFFBE9207),
  (Hue.yellow, Shade.s600): Color(0xFFAD8301),
  (Hue.yellow, Shade.s700): Color(0xFF8E6B01),
  (Hue.yellow, Shade.s800): Color(0xFF664D01),
  (Hue.yellow, Shade.s850): Color(0xFF503D02),
  (Hue.yellow, Shade.s900): Color(0xFF3A2D04),
  (Hue.yellow, Shade.s950): Color(0xFF241E08),

  (Hue.green, Shade.s50): Color(0xFFEDEECF),
  (Hue.green, Shade.s100): Color(0xFFDDE2B2),
  (Hue.green, Shade.s150): Color(0xFFCDD597),
  (Hue.green, Shade.s200): Color(0xFFBEC97E),
  (Hue.green, Shade.s300): Color(0xFFA0AF54),
  (Hue.green, Shade.s400): Color(0xFF879A39),
  (Hue.green, Shade.s500): Color(0xFF768D21),
  (Hue.green, Shade.s600): Color(0xFF66800B),
  (Hue.green, Shade.s700): Color(0xFF536907),
  (Hue.green, Shade.s800): Color(0xFF3D4C07),
  (Hue.green, Shade.s850): Color(0xFF313D07),
  (Hue.green, Shade.s900): Color(0xFF252D09),
  (Hue.green, Shade.s950): Color(0xFF1A1E0C),

  (Hue.cyan, Shade.s50): Color(0xFFDDF1E4),
  (Hue.cyan, Shade.s100): Color(0xFFBFE8D9),
  (Hue.cyan, Shade.s150): Color(0xFFA2DECE),
  (Hue.cyan, Shade.s200): Color(0xFF87D3C3),
  (Hue.cyan, Shade.s300): Color(0xFF5ABDAC),
  (Hue.cyan, Shade.s400): Color(0xFF3AA99F),
  (Hue.cyan, Shade.s500): Color(0xFF2F968D),
  (Hue.cyan, Shade.s600): Color(0xFF24837B),
  (Hue.cyan, Shade.s700): Color(0xFF1C6C66),
  (Hue.cyan, Shade.s800): Color(0xFF164F4A),
  (Hue.cyan, Shade.s850): Color(0xFF143F3C),
  (Hue.cyan, Shade.s900): Color(0xFF122F2C),
  (Hue.cyan, Shade.s950): Color(0xFF101F1D),

  (Hue.blue, Shade.s50): Color(0xFFE1ECEB),
  (Hue.blue, Shade.s100): Color(0xFFC6DDE8),
  (Hue.blue, Shade.s150): Color(0xFFABCFE2),
  (Hue.blue, Shade.s200): Color(0xFF92BFDB),
  (Hue.blue, Shade.s300): Color(0xFF66A0C8),
  (Hue.blue, Shade.s400): Color(0xFF4385BE),
  (Hue.blue, Shade.s500): Color(0xFF3171B2),
  (Hue.blue, Shade.s600): Color(0xFF205EA6),
  (Hue.blue, Shade.s700): Color(0xFF1A4F8C),
  (Hue.blue, Shade.s800): Color(0xFF163B66),
  (Hue.blue, Shade.s850): Color(0xFF133051),
  (Hue.blue, Shade.s900): Color(0xFF12253B),
  (Hue.blue, Shade.s950): Color(0xFF101A24),

  (Hue.purple, Shade.s50): Color(0xFFF0EAEC),
  (Hue.purple, Shade.s100): Color(0xFFE2D9E9),
  (Hue.purple, Shade.s150): Color(0xFFD3CAE6),
  (Hue.purple, Shade.s200): Color(0xFFC4B9E0),
  (Hue.purple, Shade.s300): Color(0xFFA699D0),
  (Hue.purple, Shade.s400): Color(0xFF8B7EC8),
  (Hue.purple, Shade.s500): Color(0xFF735EB5),
  (Hue.purple, Shade.s600): Color(0xFF5E409D),
  (Hue.purple, Shade.s700): Color(0xFF4F3685),
  (Hue.purple, Shade.s800): Color(0xFF3C2A62),
  (Hue.purple, Shade.s850): Color(0xFF31234E),
  (Hue.purple, Shade.s900): Color(0xFF261C39),
  (Hue.purple, Shade.s950): Color(0xFF1A1623),

  (Hue.magenta, Shade.s50): Color(0xFFFEE4E5),
  (Hue.magenta, Shade.s100): Color(0xFFFCCFDA),
  (Hue.magenta, Shade.s150): Color(0xFFF9B9CF),
  (Hue.magenta, Shade.s200): Color(0xFFF4A4C2),
  (Hue.magenta, Shade.s300): Color(0xFFE47DA8),
  (Hue.magenta, Shade.s400): Color(0xFFCE5D97),
  (Hue.magenta, Shade.s500): Color(0xFFB74583),
  (Hue.magenta, Shade.s600): Color(0xFFA02F6F),
  (Hue.magenta, Shade.s700): Color(0xFF87285E),
  (Hue.magenta, Shade.s800): Color(0xFF641F46),
  (Hue.magenta, Shade.s850): Color(0xFF4F1B39),
  (Hue.magenta, Shade.s900): Color(0xFF39172B),
  (Hue.magenta, Shade.s950): Color(0xFF24131D),
};
