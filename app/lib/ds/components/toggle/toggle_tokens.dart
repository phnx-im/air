// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the toggle, per density.
///
/// Geometry, motion, and the disabled dim only: colors come from the
/// palette at paint time.
@immutable
class ToggleTokens {
  const ToggleTokens({
    required this.trackWidth,
    required this.trackHeight,
    required this.thumbSize,
    required this.thumbPadding,
    required this.motion,
    required this.disabledAlpha,
  });

  final double trackWidth;
  final double trackHeight;

  /// Diameter of the thumb.
  final double thumbSize;

  /// Inset of the thumb from the track on every side, so it also sets how far
  /// the thumb travels.
  final double thumbPadding;

  /// Timing of the thumb slide and the track color change.
  final MotionPreset motion;

  /// Track and thumb both carry the state, and the thumb needs the track's
  /// contrast to stay visible, so a disabled toggle dims as a whole rather
  /// than per layer.
  final double disabledAlpha;

  static const ToggleTokens phone = ToggleTokens(
    trackWidth: S.s56,
    trackHeight: S.s32,
    thumbSize: S.s24,
    thumbPadding: S.s4,
    motion: MotionPreset.short,
    disabledAlpha: Alpha.a80,
  );

  /// Denser than [phone]: a pointer hits the smaller track reliably.
  static const ToggleTokens desktop = ToggleTokens(
    trackWidth: S.s48,
    trackHeight: S.s28,
    thumbSize: S.s20,
    thumbPadding: S.s4,
    motion: MotionPreset.short,
    disabledAlpha: Alpha.a80,
  );

  static ToggleTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
