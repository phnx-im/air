// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

import 'package:air/ds/foundations/semantic_colors.dart';

/// Elevation tier -- drives stacked drop-shadow lists.
enum Elevation { flat, small, medium, large }

/// Backdrop-blur radius tier (in logical pixels).
enum BlurLevel { thin, medium, thick }

extension BlurLevelRadius on BlurLevel {
  /// Backdrop-blur radius in logical pixels. Theme-independent.
  double get radius => switch (this) {
    BlurLevel.thin => 20,
    BlurLevel.medium => 40,
    BlurLevel.thick => 80,
  };
}

/// Named motion duration presets. All share [Effect.easeOutQuart].
/// [none] is 0ms -- for opting a state-change out of animation entirely
/// (e.g. reduce-motion fallback, or pre-mount initial values).
enum MotionPreset {
  none,
  instant,
  short,
  regular,
  medium,
  long,
  extralong,
  slomo,
}

extension MotionPresetDuration on MotionPreset {
  /// Animation duration. Theme-independent. Pair with [Effect.easeOutQuart].
  Duration get duration => switch (this) {
    MotionPreset.none => Duration.zero,
    MotionPreset.instant => const Duration(milliseconds: 50),
    MotionPreset.short => const Duration(milliseconds: 150),
    MotionPreset.regular => const Duration(milliseconds: 300),
    MotionPreset.medium => const Duration(milliseconds: 450),
    MotionPreset.long => const Duration(milliseconds: 600),
    MotionPreset.extralong => const Duration(milliseconds: 750),
    MotionPreset.slomo => const Duration(milliseconds: 1000),
  };
}

/// Drop shadows, backdrop-blur radii, and motion timings.
///
/// Static where the other resolved foundations are instances: the shadow tint
/// is the mode-invariant neutral black, and blur and motion carry no theme
/// dependency at all, so nothing here varies with brightness.
abstract final class Effect {
  /// Stacked drop shadows for the given elevation.
  static List<BoxShadow> elevation(Elevation e) => _shadows[e]!;

  /// Backdrop-blur radius in logical pixels. Pair with a translucent fill
  /// from [BackgroundMaterial] to produce the frosted-glass look used by the
  /// tab bar, plus button, and message composer.
  static double blur(BlurLevel l) => l.radius;

  /// Animation duration for a named preset. Pair with [easeOutQuart].
  static Duration duration(MotionPreset p) => p.duration;

  /// House easing curve. Theme-independent.
  static const Cubic easeOutQuart = Cubic(0.25, 1, 0.5, 1);

  static final Map<Elevation, List<BoxShadow>> _shadows = _resolveShadows();
}

// Elevation layer data. Each layer is [yOffset, blur, spread, alpha]. Inset
// layers from the source data are dropped, as Flutter's BoxShadow has no
// inset mode.

const Map<Elevation, List<List<num>>> _elevationLayers = {
  Elevation.flat: [
    [18, 5, 0, 0],
    [10, 5, 0, 0.01],
    [8, 4, 0, 0.04],
    [2, 2, 0, 0.08],
    [0, 1, 0, 0.1],
  ],
  Elevation.small: [
    [53, 15, 0, 0],
    [34, 14, 0, 0.01],
    [19, 12, 0, 0.04],
    [9, 9, 0, 0.07],
    [2, 5, 0, 0.08],
  ],
  Elevation.medium: [
    [133, 37, 0, 0],
    [85, 34, 0, 0.01],
    [48, 29, 0, 0.05],
    [21, 21, 0, 0.09],
    [5, 12, 0, 0.1],
  ],
  Elevation.large: [
    [178, 50, 0, 0],
    [114, 46, 0, 0.01],
    [64, 38, 0, 0.05],
    [28, 28, 0, 0.09],
    [7, 16, 0, 0.1],
  ],
};

/// Layers at or below this alpha are dropped: each one costs its own shadow
/// pass, and spread over the layer's blur radius the contribution does not
/// read on screen.
const double _shadowAlphaFloor = 0.01;

Map<Elevation, List<BoxShadow>> _resolveShadows() {
  // Shadows stay pure black across modes and color themes.
  const tint = Color(0xFF000000);
  final r = (tint.r * 255).round();
  final g = (tint.g * 255).round();
  final b = (tint.b * 255).round();
  return {
    for (final entry in _elevationLayers.entries)
      entry.key: [
        for (final layer in entry.value)
          if (layer[3] > _shadowAlphaFloor)
            BoxShadow(
              offset: Offset(0, layer[0].toDouble()),
              blurRadius: layer[1].toDouble(),
              spreadRadius: layer[2].toDouble(),
              color: Color.fromRGBO(r, g, b, layer[3].toDouble()),
            ),
      ],
  };
}
