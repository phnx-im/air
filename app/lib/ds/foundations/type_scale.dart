// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart';

/// Regular vs emphasized weight. Each [TypeStyleToken] carries both weights
/// pre-resolved (per the per-category-per-size rules), so the call site just
/// picks one.
enum Weight { regular, emphasized }

/// One resolved row of the typescale: size + line-height + letter spacing +
/// regular/emphasized weights. Pre-baked by [TypeScale.forPlatform]. A widget
/// that wants a `TextStyle` with a color calls [style] at paint time.
@immutable
class TypeStyleToken {
  const TypeStyleToken({
    required this.fontSize,
    required this.step,
    this.lineHeight,
    this.lineHeightRatio,
    this.letterSpacing = 0,
    required this.regularWeight,
    required this.emphasizedWeight,
  });

  final double fontSize;
  final int step;
  final double? lineHeight;
  final double? lineHeightRatio;
  final double letterSpacing;
  final FontWeight regularWeight;
  final FontWeight emphasizedWeight;

  double? get _height =>
      lineHeightRatio ?? (lineHeight != null ? lineHeight! / fontSize : null);
  double? get _ls => letterSpacing != 0 ? letterSpacing : null;

  /// Resolved line height in logical pixels, for layout code that needs to
  /// reserve space for a line of text rather than style it.
  double get lineHeightPx => lineHeight ?? fontSize * (lineHeightRatio ?? 1.0);

  /// This token as a concrete `TextStyle`.
  ///
  /// [tight] drops the token's leading so the line box hugs the glyphs, for
  /// single-line text whose height is set by something else, like a pill's
  /// padding or a row's slot.
  TextStyle style({
    Color? color,
    Weight weight = Weight.regular,
    bool tight = false,
  }) => TextStyle(
    fontSize: fontSize,
    fontWeight: weight == Weight.emphasized ? emphasizedWeight : regularWeight,
    color: color,
    height: tight ? 1.0 : _height,
    letterSpacing: _ls,
  );
}

@immutable
class TypeBody {
  const TypeBody({
    required this.mini,
    required this.xs,
    required this.s,
    required this.regular,
    required this.m,
    required this.l,
  });
  final TypeStyleToken mini, xs, s, regular, m, l;
}

@immutable
class TypeHeader {
  const TypeHeader({
    required this.xs,
    required this.s,
    required this.regular,
    required this.m,
    required this.l,
    required this.xl,
  });
  final TypeStyleToken xs, s, regular, m, l, xl;
}

/// Emoji sub-bundle. Fixed-line-height steps intended for emoji glyphs that
/// sit alongside text (reactions, stickers, large status indicators).
@immutable
class TypeEmoji {
  const TypeEmoji({required this.l, required this.jumbo});

  /// Large inline emoji (reactions, stickers, status indicators).
  final TypeStyleToken l;

  /// Jumbo emoji -- the emoji-only message glyph, sized to the typescale max.
  final TypeStyleToken jumbo;
}

/// Fully-resolved typography for one [TargetPlatform].
@immutable
class TypeScale {
  const TypeScale({
    required this.body,
    required this.header,
    required this.emoji,
  });

  final TypeBody body;
  final TypeHeader header;
  final TypeEmoji emoji;

  /// Build a typography bundle for the given operating system.
  factory TypeScale.forPlatform(TargetPlatform os) {
    final defaults = TypeScaleConfig.forPlatform(os);
    return TypeScale.build(
      os: os,
      base: defaults.base,
      min: defaults.min,
      max: defaults.max,
    );
  }

  /// Default per-step letter-spacing (tracking) in logical pixels for iOS and
  /// macOS.
  static Map<int, double> defaultKerningFor(TargetPlatform os) => switch (os) {
    TargetPlatform.iOS => _iosKerning,
    TargetPlatform.macOS => _macOSKerning,
    _ => const <int, double>{},
  };

  /// Build a typography bundle from explicit base/min/max + optional kerning.
  factory TypeScale.build({
    required TargetPlatform os,
    required double base,
    required double min,
    required double max,
    Map<int, double>? kerning,
  }) {
    final scale = _generateScale(base, min, max);
    final k = kerning ?? defaultKerningFor(os);

    // Line heights are a percentage of the font size in the design vocabulary
    // (body 130, header 120, emoji 100), so tokens name the percentage.
    // [fixedLineHeight] snaps the result to whole pixels instead, which keeps
    // an emoji box from landing on a half-pixel baseline.
    TypeStyleToken token({
      required int step,
      required int lineHeightPercent,
      bool fixedLineHeight = false,
      int regular = 400,
      int emphasized = 700,
    }) {
      final fontSize = scale[step]!;
      final ratio = lineHeightPercent / 100;
      return TypeStyleToken(
        fontSize: fontSize,
        step: step,
        lineHeight: fixedLineHeight ? (fontSize * ratio).roundToDouble() : null,
        lineHeightRatio: fixedLineHeight ? null : ratio,
        letterSpacing: k[step] ?? 0,
        regularWeight: _fontWeight(regular),
        emphasizedWeight: _fontWeight(emphasized),
      );
    }

    return TypeScale(
      body: TypeBody(
        mini: token(step: -3, lineHeightPercent: 130, emphasized: 700),
        xs: token(step: -2, lineHeightPercent: 130, emphasized: 700),
        s: token(step: -1, lineHeightPercent: 130, emphasized: 700),
        regular: token(step: 0, lineHeightPercent: 130, emphasized: 600),
        m: token(step: 1, lineHeightPercent: 130, emphasized: 600),
        l: token(step: 2, lineHeightPercent: 130, emphasized: 600),
      ),
      header: TypeHeader(
        xs: token(step: 0, lineHeightPercent: 120, emphasized: 600),
        s: token(step: 1, lineHeightPercent: 120, emphasized: 600),
        regular: token(step: 2, lineHeightPercent: 120, emphasized: 600),
        m: token(
          step: 3,
          lineHeightPercent: 120,
          regular: 300,
          emphasized: 500,
        ),
        l: token(
          step: 4,
          lineHeightPercent: 120,
          regular: 300,
          emphasized: 500,
        ),
        xl: token(
          step: 5,
          lineHeightPercent: 120,
          regular: 300,
          emphasized: 500,
        ),
      ),
      emoji: TypeEmoji(
        l: token(step: 8, lineHeightPercent: 100, fixedLineHeight: true),
        jumbo: token(step: 10, lineHeightPercent: 100, fixedLineHeight: true),
      ),
    );
  }
}

/// Default base / min / max font sizes for a [TargetPlatform]'s typescale.
@immutable
class TypeScaleConfig {
  const TypeScaleConfig({
    required this.base,
    required this.min,
    required this.max,
  });
  final double base;
  final double min;
  final double max;

  /// Per-OS body baseline, seeded from each platform's HIG/design guidelines.
  static TypeScaleConfig forPlatform(TargetPlatform os) => switch (os) {
    TargetPlatform.iOS => const TypeScaleConfig(base: 17, min: 12, max: 48),
    TargetPlatform.android ||
    TargetPlatform.fuchsia => const TypeScaleConfig(base: 16, min: 12, max: 48),
    TargetPlatform.macOS ||
    TargetPlatform.windows ||
    TargetPlatform.linux => const TypeScaleConfig(base: 14, min: 11, max: 40),
  };
}

/// The typescale for the current platform. A cache rather than a `final` so
/// the scale follows [defaultTargetPlatform] instead of freezing whichever
/// platform happened to be current at first access.
TypeScale get typeScale => _scaleCache.putIfAbsent(
  defaultTargetPlatform,
  () => TypeScale.forPlatform(defaultTargetPlatform),
);

final Map<TargetPlatform, TypeScale> _scaleCache = {};

/// Geometric typescale: separate ratios above and below base.
/// Steps below 0 interpolate from `min` (at minStep) to `base` (at 0).
/// Steps above 0 interpolate from `base` (at 0) to `max` (at maxStep).
Map<int, double> _generateScale(
  double base,
  double min,
  double max, {
  int minStep = -3,
  int maxStep = 10,
}) {
  final result = <int, double>{};
  final downRatio = math.pow(base / min, 1 / (-minStep).toDouble());
  final upRatio = math.pow(max / base, 1 / maxStep.toDouble());
  for (var i = minStep; i <= maxStep; i++) {
    final raw = i <= 0
        ? base / math.pow(downRatio, -i)
        : base * math.pow(upRatio, i);
    result[i] = (raw * 100).round() / 100;
  }
  return result;
}

/// Default letter-spacing per typescale step (pixels), tuned for SF Pro at
/// touch density. Steps +7 and above are absent and so resolve to 0.
/// Returned by [TypeScale.defaultKerningFor] for [TargetPlatform.iOS].
const Map<int, double> _iosKerning = {
  -3: -0.3,
  -2: -0.3,
  -1: -0.4,
  0: -0.6,
  1: -1.0,
  2: -0.2,
  3: -0.2,
  4: -0.2,
  5: -0.1,
  6: -0.1,
};

/// macOS tracking values -- same SF Pro table as iOS, with steps +2 and
/// +3 tightened (header.regular and header.m at desktop density need more
/// aggressive negative tracking than their iOS counterparts).
const Map<int, double> _macOSKerning = {
  -3: -0.3,
  -2: -0.3,
  -1: -0.4,
  0: -0.6,
  1: -0.9,
  2: -0.9,
  3: -1.0,
  4: -0.2,
  5: -0.1,
  6: -0.1,
};

FontWeight _fontWeight(int w) => switch (w) {
  100 => .w100,
  200 => .w200,
  300 => .w300,
  400 => .w400,
  500 => .w500,
  600 => .w600,
  700 => .w700,
  800 => .w800,
  900 => .w900,
  _ => .w400,
};
