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
    required this.tabPadding,
    required this.avatarSize,
    required this.removeGap,
  });

  final EdgeInsets tabPadding;

  final double avatarSize;

  /// Gap between the remove action and the glyph it takes back.
  final double removeGap;

  /// Inset around the section selector. The rows below carry their own
  /// padding, so the strip repeats it to line the two up.
  static const EdgeInsets tabStripPadding = EdgeInsets.symmetric(
    horizontal: S.s16,
  );

  /// Gap between two neighbouring tabs.
  static const double tabGap = S.s4;

  /// Gap between a tab's glyph and the count beside it.
  static const double tabCountGap = S.s4;

  static const double tabStripBottomGap = S.s8;

  static const double tabRadius = CornerRadius.full;

  /// Width of the fade over the trailing edge of the tab strip, wide enough
  /// for a whole emoji tab to dissolve rather than get cut off.
  static const double tabFadeWidth = S.s32;

  static const ReactionDetailsTokens phone = ReactionDetailsTokens(
    tabPadding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    avatarSize: S.s40,
    removeGap: S.s8,
  );

  /// Denser than [phone]: the tabs tighten up and the avatar drops a step, so
  /// the viewer fits more reactors in the dialog it opens in.
  static const ReactionDetailsTokens desktop = ReactionDetailsTokens(
    tabPadding: EdgeInsets.symmetric(horizontal: S.s8, vertical: S.s4),
    avatarSize: S.s32,
    removeGap: S.s12,
  );

  static ReactionDetailsTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
