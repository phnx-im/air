// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// Named size tokens, valid for layout: gaps, padding, and box dimensions.
abstract final class S {
  static const double s0 = 0.0;
  static const double s1 = 1.0;
  static const double s2 = 2.0;
  static const double s4 = 4.0;
  static const double s8 = 8.0;
  static const double s12 = 12.0;
  static const double s16 = 16.0;
  static const double s20 = 20.0;
  static const double s24 = 24.0;
  static const double s28 = 28.0;
  static const double s32 = 32.0;
  static const double s40 = 40.0;
  static const double s48 = 48.0;
  static const double s56 = 56.0;
  static const double s64 = 64.0;
  static const double s72 = 72.0;
  static const double s80 = 80.0;
  static const double s96 = 96.0;
  static const double s120 = 120.0;
  static const double s128 = 128.0;
  static const double s160 = 160.0;
  static const double s192 = 192.0;
  static const double s240 = 240.0;
}

/// Named corner radius tokens.
abstract final class Radii {
  static const double px0 = 0.0;
  static const double px1 = 1.0;
  static const double px2 = 2.0;
  static const double px4 = 4.0;
  static const double px8 = 8.0;
  static const double px12 = 12.0;
  static const double px16 = 16.0;
  static const double px20 = 20.0;
  static const double px24 = 24.0;
  static const double px28 = 28.0;
  static const double px32 = 32.0;

  /// Pill and circle sentinel. Flutter scales an oversized radius down to fit
  /// the box.
  static const double full = 1000.0;
}

/// Named stroke widths: borders, dividers, and painted outlines.
abstract final class Strokes {
  static const double px0 = 0.0;
  static const double px0_5 = 0.5;
  static const double px1 = 1.0;
  static const double px1_5 = 1.5;
  static const double px2 = 2.0;
  static const double px4 = 4.0;
  static const double px8 = 8.0;
}

/// Named opacity tokens as 0-1 alphas.
abstract final class Opacities {
  static const double alpha0 = 0.0;
  static const double alpha5 = 0.05;
  static const double alpha10 = 0.10;
  static const double alpha15 = 0.15;
  static const double alpha20 = 0.20;
  static const double alpha40 = 0.40;
  static const double alpha50 = 0.50;
  static const double alpha60 = 0.60;
  static const double alpha80 = 0.80;
  static const double alpha85 = 0.85;
  static const double alpha90 = 0.90;
  static const double alpha95 = 0.95;
  static const double alpha100 = 1.0;
}
