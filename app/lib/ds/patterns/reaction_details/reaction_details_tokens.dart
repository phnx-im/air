// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the reaction viewer, per density.
///
/// Geometry only: colors come from the palette at paint time, and the
/// surface the viewer sits on is the host sheet's to supply.
@immutable
class ReactionDetailsTokens {
  const ReactionDetailsTokens({
    required this.tabStripPadding,
    required this.tabPadding,
    required this.tabGap,
    required this.tabCountGap,
    required this.tabStripBottomGap,
    required this.avatarSize,
    required this.removeGap,
  });

  /// Inset around the section selector. The rows below carry their own
  /// padding, so the strip repeats it to line the two up.
  final EdgeInsets tabStripPadding;

  final EdgeInsets tabPadding;

  /// Gap between two neighbouring tabs.
  final double tabGap;

  /// Gap between a tab's glyph and the count beside it.
  final double tabCountGap;

  final double tabStripBottomGap;

  final double avatarSize;

  /// Gap between the remove action and the glyph it takes back.
  final double removeGap;

  static const double tabRadius = CornerRadius.full;

  /// Width of the fade over the trailing edge of the tab strip, wide enough
  /// for a whole emoji tab to dissolve rather than get cut off.
  static const double tabFadeWidth = S.s32;

  static const ReactionDetailsTokens phone = ReactionDetailsTokens(
    tabStripPadding: EdgeInsets.symmetric(horizontal: S.s16),
    tabPadding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    tabGap: S.s4,
    tabCountGap: S.s4,
    tabStripBottomGap: S.s8,
    avatarSize: S.s40,
    removeGap: S.s8,
  );

  /// Denser than [phone]: the tabs tighten up and the avatar drops a step, so
  /// the viewer fits more reactors in the dialog it opens in.
  static const ReactionDetailsTokens desktop = ReactionDetailsTokens(
    tabStripPadding: EdgeInsets.symmetric(horizontal: S.s16),
    tabPadding: EdgeInsets.symmetric(horizontal: S.s8, vertical: S.s4),
    tabGap: S.s4,
    tabCountGap: S.s4,
    tabStripBottomGap: S.s8,
    avatarSize: S.s32,
    removeGap: S.s12,
  );

  static ReactionDetailsTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
