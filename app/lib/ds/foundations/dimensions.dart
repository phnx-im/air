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
abstract final class CornerRadius {
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
abstract final class StrokeWidth {
  static const double px0 = 0.0;
  static const double px0_5 = 0.5;
  static const double px1 = 1.0;
  static const double px1_5 = 1.5;
  static const double px2 = 2.0;
  static const double px4 = 4.0;
  static const double px8 = 8.0;
}

/// Named opacity tokens as 0-1 alphas.
abstract final class Alpha {
  static const double a0 = 0.0;
  static const double a5 = 0.05;
  static const double a10 = 0.10;
  static const double a15 = 0.15;
  static const double a20 = 0.20;
  static const double a40 = 0.40;
  static const double a50 = 0.50;
  static const double a60 = 0.60;
  static const double a80 = 0.80;
  static const double a85 = 0.85;
  static const double a90 = 0.90;
  static const double a95 = 0.95;
  static const double a100 = 1.0;
}
