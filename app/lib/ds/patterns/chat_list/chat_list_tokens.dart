// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scroll/scroll_fade_tokens.dart';
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
    headerClearance: S.s80,
    contentBottomPadding: S.s160,
    fades: ChatListFadeTokens.phone,
  );

  static const ChatListTokens desktop = ChatListTokens(
    headerClearance: S.s0,
    contentBottomPadding: S.s48,
    fades: ChatListFadeTokens.desktop,
  );

  static ChatListTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
