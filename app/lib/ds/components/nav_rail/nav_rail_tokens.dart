// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';

/// Layout tokens for the desktop navigation rail.
abstract final class NavRailTokens {
  static const double width = S.s80;

  /// Side of one square nav cell.
  static const double itemSize = S.s64;
  static const double itemRadius = CornerRadius.px12;
  static const double itemGap = S.s4;

  static const double paddingTop = S.s8;

  /// Space reserved above the first cell for native window controls that float
  /// over the rail's top-left. Applied only where those controls exist, see
  /// `NavRail.reserveWindowControls`.
  static const double windowControlsInset = S.s32;

  static const double iconSize = S.s24;
  static const double avatarSize = S.s28;
  static const double labelGap = S.s4;

  /// Pitch from one cell's top edge to the next.
  static const double stride = itemSize + itemGap;
}
