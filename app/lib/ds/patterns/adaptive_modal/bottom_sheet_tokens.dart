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
@immutable
class BottomSheetTokens {
  const BottomSheetTokens({
    required this.topRadius,
    required this.handleWidth,
    required this.handleHeight,
    required this.handleGap,
    required this.contentPadding,
    required this.enter,
    required this.exit,
    required this.dragSnapBack,
    required this.dragDismissVelocity,
    required this.dragDismissDistance,
  });

  /// Radius of the two top corners. The bottom edge is flush with the screen,
  /// so it stays square.
  final double topRadius;

  /// The grab handle rides on the scrim above the card rather than inside it,
  /// so the card's top edge stays one unbroken line.
  final double handleWidth;
  final double handleHeight;

  /// Gap below the handle, mirrored above it so the drag target stays
  /// symmetric around the pill.
  final double handleGap;

  /// Inset the card gives its child. We add the keyboard and home-indicator
  /// insets to the bottom at paint time.
  final EdgeInsets contentPadding;

  /// Arrival is slower than departure: the sheet has to announce itself on the
  /// way in, but once dismissed it's only in the way.
  final MotionPreset enter;
  final MotionPreset exit;

  /// Return trip after a drag that didn't reach the dismiss threshold.
  final MotionPreset dragSnapBack;

  /// Downward fling speed, in logical pixels per second, past which the sheet
  /// dismisses however short the drag was.
  final double dragDismissVelocity;

  /// Drag distance, in logical pixels, past which releasing dismisses. Far
  /// enough that resting a thumb on the handle doesn't close the sheet.
  final double dragDismissDistance;

  static const BottomSheetTokens standard = BottomSheetTokens(
    topRadius: CornerRadius.px20,
    handleWidth: S.s64,
    handleHeight: S.s4,
    handleGap: S.s8,
    contentPadding: EdgeInsets.fromLTRB(S.s24, S.s32, S.s24, S.s8),
    enter: MotionPreset.regular,
    exit: MotionPreset.short,
    dragSnapBack: MotionPreset.short,
    dragDismissVelocity: 700,
    dragDismissDistance: 120,
  );

  /// Vertical space the handle claims above the card, including the gap on
  /// both sides of the pill.
  double get handleExtent => handleHeight + handleGap * 2;
}
