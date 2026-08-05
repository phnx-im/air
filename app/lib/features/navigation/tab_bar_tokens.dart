// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';
import 'package:air/ds/foundations/foundations.dart';

/// Layout tokens for the floating mobile tab bar.
abstract final class TabBarTokens {
  static const double height = S.s64;
  static const double tabWidth = S.s120;
  static const double pillRadius = CornerRadius.full;
  static const double paddingHorizontal = S.s32;
  static const double paddingBottom = S.s32;

  /// Gap between tabs. Negative, so the tabs lap over each other: the active
  /// tab is drawn frontmost, and tapping the one behind slides the front pill
  /// onto it.
  static const double tabGap = -S.s8;

  /// How far the active pill sits inside the bar on every side, leaving a ring
  /// of bar background around it.
  static const double activePillInset = StrokeWidth.px1;

  static const double iconSize = S.s20;
  static const double avatarSize = S.s24;
  static const double labelGap = S.s4;

  static const EdgeInsets tabPadding = EdgeInsets.symmetric(
    horizontal: S.s16,
    vertical: S.s8,
  );

  /// Pitch from one tab's left edge to the next.
  static const double stride = tabWidth + tabGap;

  /// Width of a bar holding [tabCount] tabs, accounting for the overlap.
  static double barWidth(int tabCount) =>
      tabWidth * tabCount + tabGap * (tabCount - 1);

  /// Bottom inset that content behind the floating tab bar should reserve so
  /// it stays scrollable past the bar.
  static double bottomInset(BuildContext context) {
    final safeBottom = MediaQuery.paddingOf(context).bottom;
    return height + (safeBottom > paddingBottom ? safeBottom : paddingBottom);
  }
}
