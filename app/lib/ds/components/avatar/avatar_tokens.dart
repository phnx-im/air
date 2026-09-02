// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for an avatar, plus the one palette a component owns outright.
///
/// The fallback gradients are a decorative scale rather than a semantic, so
/// they read primitives directly and stay put in either brightness.
abstract final class AvatarTokens {
  /// Initial-letter size as a fraction of the circle's diameter. Call sites
  /// size avatars anywhere from a tab-bar glyph to a profile header, so the
  /// letter tracks the circle instead of stepping through fixed tiers.
  static const double letterRatio = 0.5;

  static const Alignment gradientBegin = Alignment.topLeft;
  static const Alignment gradientEnd = Alignment.bottomRight;

  static double letterSize(double diameter) => diameter * letterRatio;

  /// The fallback gradient [seed] hashes onto, in the hues of [primitives].
  static LinearGradient gradientFor(String? seed, PrimitivePalette primitives) {
    final hue = Hue.values[_hueIndex(seed)];
    return LinearGradient(
      colors: [
        primitives.chromatic(hue, _startShade),
        primitives.chromatic(hue, _endShade),
      ],
      begin: gradientBegin,
      end: gradientEnd,
    );
  }

  static const Shade _startShade = Shade.s300;
  static const Shade _endShade = Shade.s700;

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
    return hash % Hue.values.length;
  }
}
