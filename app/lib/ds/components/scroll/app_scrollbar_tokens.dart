// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry of the scrollbar thumb, per density.
///
/// Geometry only: colors come from the palette at paint time.
@immutable
class AppScrollbarTokens {
  const AppScrollbarTokens({
    required this.width,
    required this.minThumbHeight,
    required this.radius,
    required this.trackInset,
    required this.rightInset,
  });

  final double width;

  /// Floor for the thumb's height. A long enough history drives the
  /// proportional height towards nothing, and a thumb too short to read as a
  /// position marker is worse than one that lies slightly about the extent.
  final double minThumbHeight;

  final double radius;

  /// Clearance the track keeps at both ends, on top of whatever the host
  /// reserves for chrome floating over the scrollable.
  final double trackInset;

  /// Gap between the thumb and the right edge of the viewport.
  final double rightInset;

  /// Dwell between the content settling and the thumb starting to fade, so a
  /// scroll broken into several flicks doesn't blink the thumb on and off.
  static const Duration hideDelay = Duration(milliseconds: 640);

  /// Fade-out duration. Appearing is instant: the thumb has to be there the
  /// frame the content starts moving.
  static const Duration hideDuration = Duration(milliseconds: 400);

  static const AppScrollbarTokens phone = AppScrollbarTokens(
    width: S.s4,
    minThumbHeight: S.s24,
    radius: CornerRadius.full,
    trackInset: S.s2,
    rightInset: S.s2,
  );

  /// Only the floor tightens: a pointer reads a shorter thumb accurately,
  /// where a fingertip needs something it could plausibly land on.
  static const AppScrollbarTokens desktop = AppScrollbarTokens(
    width: S.s4,
    minThumbHeight: S.s16,
    radius: CornerRadius.full,
    trackInset: S.s2,
    rightInset: S.s2,
  );

  static AppScrollbarTokens get current => DeviceType.isPhone ? phone : desktop;
}
