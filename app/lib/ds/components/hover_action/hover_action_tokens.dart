// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry and motion for a [HoverAction].
///
/// Geometry only: colors come from the palette at paint time. Every field
/// carries a default, so a host that only has to match the button to its own
/// metrics overrides [size] and leaves the rest alone.
@immutable
class HoverActionTokens {
  const HoverActionTokens({
    this.size = S.s32,
    this.glyphSize = _glyphSize,
    this.gap = S.s8,
    this.revealMotion = MotionPreset.short,
    this.hiddenScale = _hiddenScale,
  });

  /// Button diameter. A host that sits the button next to text sizes it off
  /// that text's metrics, so the two line up.
  final double size;

  /// Glyph diameter. Fixed here rather than taken from the pair for [size]:
  /// a metric-derived diameter lands between two steps of the scale, and the
  /// pair for the step above overpowers the button.
  final double glyphSize;

  /// Gap between two adjacent actions, and between the run and what it sits
  /// beside. The host reserves the slot, so it needs the same number: a run of
  /// n actions occupies `n * size + n * gap`.
  final double gap;

  /// Duration of the reveal. Short enough that the button is there by the time
  /// the pointer has settled.
  final MotionPreset revealMotion;

  /// Scale the button rests at while hidden. It grows into place rather than
  /// only fading, so the reveal reads as the button arriving rather than the
  /// bubble changing opacity.
  final double hiddenScale;

  static const HoverActionTokens standard = HoverActionTokens();

  static const double _glyphSize = 18;

  static const double _hiddenScale = 0.8;
}
