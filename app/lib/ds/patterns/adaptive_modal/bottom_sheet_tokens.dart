// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout and motion tokens for the bottom sheet.
///
/// Geometry only: colors come from the palette at paint time.
///
/// One set rather than a per-density pair: the sheet is the phone presentation
/// of a modal surface, and the two-pane layout shows a centered `Modal`
/// instead, so there's no desktop variant to diverge from.
abstract final class BottomSheetTokens {
  /// Radius of the two top corners. The bottom edge is flush with the screen,
  /// so it stays square.
  static const double topRadius = CornerRadius.px20;

  /// The grab handle rides on the scrim above the card rather than inside it,
  /// so the card's top edge stays one unbroken line.
  static const double handleWidth = S.s64;
  static const double handleHeight = S.s4;

  /// Gap below the handle, mirrored above it so the drag target stays
  /// symmetric around the pill.
  static const double handleGap = S.s8;

  /// Inset the card gives its child. We add the keyboard and home-indicator
  /// insets to the bottom at paint time.
  static const EdgeInsets contentPadding = EdgeInsets.fromLTRB(
    S.s24,
    S.s32,
    S.s24,
    S.s8,
  );

  /// Arrival is slower than departure: the sheet has to announce itself on the
  /// way in, but once dismissed it's only in the way.
  static const MotionPreset enter = MotionPreset.regular;
  static const MotionPreset exit = MotionPreset.short;

  /// Return trip after a drag that didn't reach the dismiss threshold.
  static const MotionPreset dragSnapBack = MotionPreset.short;

  /// Downward fling speed, in logical pixels per second, past which the sheet
  /// dismisses however short the drag was.
  static const double dragDismissVelocity = 700;

  /// Drag distance, in logical pixels, past which releasing dismisses. Far
  /// enough that resting a thumb on the handle doesn't close the sheet.
  static const double dragDismissDistance = 120;

  /// Vertical space the handle claims above the card, including the gap on
  /// both sides of the pill.
  static double get handleExtent => handleHeight + handleGap * 2;
}
