// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// Layout tokens for the floating mobile tab bar.
abstract final class TabBarTokens {
  static const double height = 72;
  static const double tabWidth = 128;
  static const double pillRadius = 1000;
  static const double paddingHorizontal = 32;
  static const double paddingBottom = 32;

  static const double iconSize = 24;
  static const double avatarSize = 28;
  static const double labelGap = 8;

  /// Vertical space reserved for the floating tab bar above the system safe
  /// area. Use [bottomInset] when the system safe area should also be
  /// accounted for.
  static const double footprint = height + paddingBottom;

  /// Bottom inset that content behind the floating tab bar should reserve so
  /// it stays scrollable past the bar.
  static double bottomInset(BuildContext context) {
    final safeBottom = MediaQuery.paddingOf(context).bottom;
    return height + (safeBottom > paddingBottom ? safeBottom : paddingBottom);
  }
}
