// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';

/// Timings for the delivery glyph.
///
/// No geometry of its own: the glyph size is the host's, from its own bundle,
/// since a stamp under a bubble and a list gutter weigh the glyph differently.
abstract final class DeliveryStatusTokens {
  /// A message in flight shows nothing until this elapses, so a send that
  /// lands right away never flashes a spinner.
  static const Duration sendingReveal = Duration(seconds: 2);

  /// One full turn of the in-flight spinner.
  static const Duration spinnerPeriod = Duration(seconds: 1);

  /// Crossfade as one delivery state replaces another.
  static const MotionPreset motion = MotionPreset.short;
}
