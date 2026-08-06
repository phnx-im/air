// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for an avatar, plus the one palette a component owns outright.
///
/// The fallback gradients are a fixed decorative scale rather than a themeable
/// semantic, so they read primitives directly and stay put in either theme.
abstract final class AvatarTokens {
  /// Initial-letter size as a fraction of the circle's diameter. Call sites
  /// size avatars anywhere from a tab-bar glyph to a profile header, so the
  /// letter tracks the circle instead of stepping through fixed tiers.
  static const double letterRatio = 0.5;

  static const Alignment gradientBegin = Alignment.topLeft;
  static const Alignment gradientEnd = Alignment.bottomRight;

  static double letterSize(double diameter) => diameter * letterRatio;

  /// The fallback gradient [seed] hashes onto.
  static LinearGradient gradientFor(String? seed) => LinearGradient(
    colors: _hueGradients[_hueIndex(seed)],
    begin: gradientBegin,
    end: gradientEnd,
  );

  static const Shade _startShade = Shade.s300;
  static const Shade _endShade = Shade.s700;

  /// One gradient per chromatic hue, in palette order. That order is
  /// load-bearing: [_hueIndex] indexes into this list, so adding a hue to
  /// [Hue] re-colors existing avatars.
  static final List<List<Color>> _hueGradients = [
    for (final hue in Hue.values)
      [
        Primitive.chromatic(hue, _startShade),
        Primitive.chromatic(hue, _endShade),
      ],
  ];

  static int _hueIndex(String? seed) {
    if (seed == null) {
      return 0;
    }
    // Cheap uniformity inspired by Java's String.hashCode()
    var hash = 0;
    for (final codeUnit in seed.codeUnits) {
      hash = ((hash << 5) + hash) + codeUnit;
      hash &= 0xFFFFFFFF;
    }
    return hash % _hueGradients.length;
  }
}
