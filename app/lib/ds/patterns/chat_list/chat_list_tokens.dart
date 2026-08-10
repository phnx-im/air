// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for the chat list's scroll frame, per density.
///
/// Geometry only: the surface color comes from the host, which is the side that
/// knows whether the list fills the screen or sits inside a panel. Embeds the
/// [ChatListFadeTokens] the frame composes.
@immutable
class ChatListTokens {
  const ChatListTokens({
    required this.headerClearance,
    required this.contentBottomPadding,
    required this.fades,
  });

  /// Space between the header and the first row. The phone's header floats over
  /// a full-bleed list and needs the room to read as separate from it. The
  /// two-pane header sits on the panel and closes the gap itself.
  final double headerClearance;

  /// Space kept below the last row. On a phone this clears the floating tab bar
  /// and its fade. The two-pane layout has neither and only needs the panel's
  /// own bottom margin.
  final double contentBottomPadding;

  final ChatListFadeTokens fades;

  static const ChatListTokens phone = ChatListTokens(
    headerClearance: S.s24,
    contentBottomPadding: S.s160,
    fades: ChatListFadeTokens.phone,
  );

  static const ChatListTokens desktop = ChatListTokens(
    headerClearance: S.s0,
    contentBottomPadding: S.s48,
    fades: ChatListFadeTokens.desktop,
  );

  static ChatListTokens get current => DeviceType.isPhone ? phone : desktop;
}

/// Fade values for the chat list, per density.
///
/// Each strip measures from its own edge of the viewport, so the top one
/// covers the status bar and the floating header without growing with the
/// notch.
@immutable
class ChatListFadeTokens {
  const ChatListFadeTokens({
    required this.topHeight,
    required this.topSolidStop,
    required this.topOpacity,
    required this.bottomHeight,
  });

  final double topHeight;

  /// Fraction of [topHeight] held at full strength before the ramp starts. The
  /// header needs an opaque bed to sit on, and the denser two-pane header needs
  /// less of one. A gradient position rather than an alpha, so it stays a
  /// literal.
  final double topSolidStop;

  /// Peak alpha at the top edge. Full-screen chrome hides what slides under the
  /// header, while the two-pane layout only dims it.
  final double topOpacity;

  final double bottomHeight;

  /// The bottom strip ramps from the very edge: it has no chrome to bed, only
  /// the last row to soften.
  static const double bottomSolidStop = 0.0;

  /// Peak alpha of the bottom strip. It only softens the last row, so it stays
  /// translucent at either density.
  static const double bottomOpacity = Alpha.a80;

  static const ChatListFadeTokens phone = ChatListFadeTokens(
    topHeight: S.s128,
    topSolidStop: 0.2,
    topOpacity: Alpha.a100,
    bottomHeight: S.s128,
  );

  static const ChatListFadeTokens desktop = ChatListFadeTokens(
    topHeight: S.s80,
    topSolidStop: 0.1,
    topOpacity: Alpha.a80,
    bottomHeight: S.s80,
  );

  static ChatListFadeTokens get current => DeviceType.isPhone ? phone : desktop;
}
