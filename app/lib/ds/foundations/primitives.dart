// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

class AppColors {
  // ── Neutral ────────────────────────────────────────────────────────────────
  static const Map<int, Color> _neutralShades = {
    0: Color(0xFFFFFFFF),
    50: Color(0xFFF3F3F3),
    100: Color(0xFFE7E7E6),
    200: Color(0xFFDADADA),
    300: Color(0xFFCECECD),
    400: Color(0xFFC1C1C0),
    500: Color(0xFFB0B0AE),
    600: Color(0xFF949391),
    700: Color(0xFF777674),
    800: Color(0xFF5A5857),
    900: Color(0xFF3C3B3B),
    950: Color(0xFF1F1E1E),
    1000: Color(0xFF000000),
  };
  static const MaterialColor neutral = MaterialColor(
    0xFFB0B0AE, // Base color [500]
    _neutralShades,
  );

  // ── Red ────────────────────────────────────────────────────────────────────
  static const Map<int, Color> _redShades = {
    50: Color(0xFFFFE1D5),
    100: Color(0xFFFCCABB),
    150: Color(0xFFFDB2A2),
    200: Color(0xFFF89A8A),
    300: Color(0xFFE8705F),
    400: Color(0xFFD14D41),
    500: Color(0xFFC03E35),
    600: Color(0xFFAF3029),
    700: Color(0xFF942822),
    800: Color(0xFF6C201C),
    850: Color(0xFF551B18),
    900: Color(0xFF3E1715),
    950: Color(0xFF261312),
  };
  static const MaterialColor red = MaterialColor(
    0xFFC03E35, // Base color [500]
    _redShades,
  );

  // ── Orange ────────────────────────────────────────────────────────────────
  static const Map<int, Color> _orangeShades = {
    50: Color(0xFFFFE7CE),
    100: Color(0xFFFED3AF),
    150: Color(0xFFFCC192),
    200: Color(0xFFF9AE77),
    300: Color(0xFFEC8B49),
    400: Color(0xFFDA702C),
    500: Color(0xFFCB6120),
    600: Color(0xFFBC5215),
    700: Color(0xFF9D4310),
    800: Color(0xFF71320D),
    850: Color(0xFF59290D),
    900: Color(0xFF40200D),
    950: Color(0xFF27180E),
  };
  static const MaterialColor orange = MaterialColor(0xFFCB6120, _orangeShades);

  // ── Yellow ────────────────────────────────────────────────────────────────
  static const Map<int, Color> _yellowShades = {
    50: Color(0xFFFAEEC6),
    100: Color(0xFFF6E2A0),
    150: Color(0xFFF1D67E),
    200: Color(0xFFECCB60),
    300: Color(0xFFDFB431),
    400: Color(0xFFD0A215),
    500: Color(0xFFBE9207),
    600: Color(0xFFAD8301),
    700: Color(0xFF8E6B01),
    800: Color(0xFF664D01),
    850: Color(0xFF503D02),
    900: Color(0xFF3A2D04),
    950: Color(0xFF241E08),
  };
  static const MaterialColor yellow = MaterialColor(
    0xFFBE9207, // Base color [500]
    _yellowShades,
  );

  // ── Green ─────────────────────────────────────────────────────────────────
  static const Map<int, Color> _greenShades = {
    50: Color(0xFFEDEECF),
    100: Color(0xFFDDE2B2),
    150: Color(0xFFCDD597),
    200: Color(0xFFBEC97E),
    300: Color(0xFFA0AF54),
    400: Color(0xFF879A39),
    500: Color(0xFF768D21),
    600: Color(0xFF66800B),
    700: Color(0xFF536907),
    800: Color(0xFF3D4C07),
    850: Color(0xFF313D07),
    900: Color(0xFF252D09),
    950: Color(0xFF1A1E0C),
  };
  static const MaterialColor green = MaterialColor(
    0xFF768D21, // Base color [500]
    _greenShades,
  );

  // ── Cyan ──────────────────────────────────────────────────────────────────
  static const Map<int, Color> _cyanShades = {
    50: Color(0xFFDDF1E4),
    100: Color(0xFFBFE8D9),
    150: Color(0xFFA2DECE),
    200: Color(0xFF87D3C3),
    300: Color(0xFF5ABDAC),
    400: Color(0xFF3AA99F),
    500: Color(0xFF2F968D),
    600: Color(0xFF24837B),
    700: Color(0xFF1C6C66),
    800: Color(0xFF164F4A),
    850: Color(0xFF143F3C),
    900: Color(0xFF122F2C),
    950: Color(0xFF101F1D),
  };
  static const MaterialColor cyan = MaterialColor(
    0xFF2F968D, // Base color [500]
    _cyanShades,
  );

  // ── Blue ──────────────────────────────────────────────────────────────────
  static const Map<int, Color> _blueShades = {
    50: Color(0xFFE1ECEB),
    100: Color(0xFFC6DDE8),
    150: Color(0xFFABCFE2),
    200: Color(0xFF92BFDB),
    300: Color(0xFF66A0C8),
    400: Color(0xFF4385BE),
    500: Color(0xFF3171B2),
    600: Color(0xFF205EA6),
    700: Color(0xFF1A4F8C),
    800: Color(0xFF163B66),
    850: Color(0xFF133051),
    900: Color(0xFF12253B),
    950: Color(0xFF101A24),
  };
  static const MaterialColor blue = MaterialColor(
    0xFF3171B2, // Base color [500]
    _blueShades,
  );

  // ── Purple ────────────────────────────────────────────────────────────────
  static const Map<int, Color> _purpleShades = {
    50: Color(0xFFF0EAEC),
    100: Color(0xFFE2D9E9),
    150: Color(0xFFD3CAE6),
    200: Color(0xFFC4B9E0),
    300: Color(0xFFA699D0),
    400: Color(0xFF8B7EC8),
    500: Color(0xFF735EB5),
    600: Color(0xFF5E409D),
    700: Color(0xFF4F3685),
    800: Color(0xFF3C2A62),
    850: Color(0xFF31234E),
    900: Color(0xFF261C39),
    950: Color(0xFF1A1623),
  };
  static const MaterialColor purple = MaterialColor(
    0xFF735EB5, // Base color [500]
    _purpleShades,
  );

  // ── Magenta ───────────────────────────────────────────────────────────────
  static const Map<int, Color> _magentaShades = {
    50: Color(0xFFFEE4E5),
    100: Color(0xFFFCCFDA),
    150: Color(0xFFF9B9CF),
    200: Color(0xFFF4A4C2),
    300: Color(0xFFE47DA8),
    400: Color(0xFFCE5D97),
    500: Color(0xFFB74583),
    600: Color(0xFFA02F6F),
    700: Color(0xFF87285E),
    800: Color(0xFF641F46),
    850: Color(0xFF4F1B39),
    900: Color(0xFF39172B),
    950: Color(0xFF24131D),
  };
  static const MaterialColor magenta = MaterialColor(
    0xFFB74583, // Base color [500]
    _magentaShades,
  );
}
