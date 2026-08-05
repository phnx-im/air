// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Fade values for the chat list, per density.
///
/// Each strip measures from its own edge of the viewport, so the top one
/// covers the status bar and the floating header without growing with the
/// notch.
@immutable
class ChatListFadeTokens {
  const ChatListFadeTokens({
    required this.topHeight,
    required this.topStop,
    required this.bottomHeight,
    required this.bottomOpacity,
  });

  final double topHeight;

  /// Fraction of [topHeight] held at full strength before the ramp starts. The
  /// header needs an opaque bed to sit on, and the denser two-pane header needs
  /// less of one.
  final double topStop;

  final double bottomHeight;

  /// Peak alpha of the bottom strip. The top fade has to hide rows sliding
  /// under the header, so it's opaque. The bottom one only softens the last
  /// row, so it stays translucent.
  final double bottomOpacity;

  static const ChatListFadeTokens phone = ChatListFadeTokens(
    topHeight: S.s96,
    topStop: 0.4,
    bottomHeight: S.s128,
    bottomOpacity: 0.8,
  );

  static const ChatListFadeTokens desktop = ChatListFadeTokens(
    topHeight: S.s80,
    topStop: 0.2,
    bottomHeight: S.s80,
    bottomOpacity: 0.8,
  );

  static ChatListFadeTokens get current => DeviceType.isPhone ? phone : desktop;
}

/// Fade values for the message list, per density.
@immutable
class MessageListFadeTokens {
  const MessageListFadeTokens({
    required this.topTail,
    required this.topOpacity,
    required this.bottomHeight,
    required this.bottomOpacity,
  });

  /// Ramp below the header bar. The strip starts at the very top of the
  /// viewport, so the caller adds the bar's height and the status bar inset: on
  /// a phone this fade does grow with the notch, unlike the chat list's.
  final double topTail;

  /// Peak alpha at the top edge. Full-screen chrome hides what slides under the
  /// bar, while the two-pane layout only dims it.
  final double topOpacity;

  final double bottomHeight;
  final double bottomOpacity;

  /// Scroll distance over which the bottom strip ramps in. At rest on the
  /// newest message there's nothing below the fold to fade.
  static const double bottomRevealDistance = S.s16;

  /// Clearance between the end of the top fade and the first row at rest.
  static const double contentTopGap = S.s8;

  static const MessageListFadeTokens phone = MessageListFadeTokens(
    topTail: S.s48,
    topOpacity: 1.0,
    bottomHeight: S.s64,
    bottomOpacity: 0.8,
  );

  static const MessageListFadeTokens desktop = MessageListFadeTokens(
    topTail: S.s24,
    topOpacity: 0.8,
    bottomHeight: S.s80,
    bottomOpacity: 0.8,
  );

  static MessageListFadeTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
