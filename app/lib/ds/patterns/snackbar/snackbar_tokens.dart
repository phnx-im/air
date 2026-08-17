// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Visual tone of a [Snackbar]. The tone picks the fill, so a call site never
/// assembles a look by hand.
enum SnackbarTone {
  /// Confirmation pill. For an action that went through and needs no
  /// follow-up.
  success,

  /// Error pill. For an action that didn't go through.
  danger,
}

/// Layout tokens for the snackbar pill, plus where it sits on screen.
///
/// Geometry only: colors come from the palette at paint time, picked by
/// [SnackbarTone].
abstract final class SnackbarTokens {
  static const double radius = CornerRadius.px8;
  static const EdgeInsets padding = EdgeInsets.symmetric(
    horizontal: S.s16,
    vertical: S.s8,
  );

  /// Caps the pill width so a long label ellipsizes rather than spanning the
  /// viewport.
  static const double maxWidth = Measure.m360;

  static const Elevation elevation = Elevation.small;

  /// Where the pill sits relative to the viewport edges. The bottom clearance
  /// carries the message composer, so a pill raised from a chat never lands on
  /// top of the input.
  static const EdgeInsets insets = EdgeInsets.only(
    left: S.s16,
    right: S.s16,
    bottom: S.s96,
  );
}
