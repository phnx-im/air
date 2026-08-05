// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for one row of a conversation, per density.
///
/// Geometry only: colors come from the palette at paint time, and the
/// bubble the row carries is the host's to supply.
@immutable
class MessageRowTokens {
  const MessageRowTokens({
    required this.padding,
    required this.avatarSize,
    required this.avatarGap,
    required this.avatarBottomNudge,
    required this.bubbleTextInset,
    required this.senderNameGap,
    required this.groupGap,
  });

  /// Inset between the row and the edges of the conversation. Horizontal only:
  /// the gaps between rows belong to the list, which knows what precedes and
  /// follows each row, see [groupGap] and [flightGap].
  final EdgeInsets padding;

  /// Diameter of the avatar column. Matches the input's leading button at the
  /// same density, so the two line up down the left edge of the screen.
  final double avatarSize;

  /// Gap between the avatar column and the bubble, mirroring the input's gap
  /// between its leading button and the field.
  final double avatarGap;

  /// Lift of the avatar off the foot of its column. It reads as sitting on the
  /// bubble's last line rather than hanging off its corner, so the nudge
  /// follows the bubble's own bottom inset and not the avatar's diameter.
  final double avatarBottomNudge;

  /// The bubble's own horizontal text inset. The row doesn't apply it directly.
  /// It only enters [contentInset], which is what lines the sender name up with
  /// the text of the bubble under it rather than with the bubble's edge.
  final double bubbleTextInset;

  /// Gap below the sender name.
  final double senderNameGap;

  /// Gap above the first row of a flight, which the list applies. Wide enough
  /// that a change of sender reads before the name does.
  final double groupGap;

  /// Gap between consecutive rows of the same flight, which the list applies.
  /// The bubbles of a flight read as one block, so this only keeps them apart.
  static const double flightGap = S.s2;

  /// The row splits into [contentFlex] parts of bubble to [gutterFlex] parts of
  /// empty gutter, so a long message stops short of the far edge and the side
  /// it's anchored to stays legible at a glance.
  ///
  /// The only cap on a bubble's width: the bubble itself hugs its content and
  /// takes whatever the row hands it.
  static const int contentFlex = 5;
  static const int gutterFlex = 1;

  /// Distance from the row's leading edge to the bubble's text. The sender name
  /// and the footer take it, so name, text, and timestamp share one margin.
  double contentInset({required bool withAvatar}) =>
      (withAvatar ? avatarSize + avatarGap : 0) + bubbleTextInset;

  static const MessageRowTokens phone = MessageRowTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s16),
    avatarSize: S.s40,
    avatarGap: S.s8,
    avatarBottomNudge: S.s4,
    bubbleTextInset: S.s12,
    senderNameGap: S.s4,
    groupGap: S.s16,
  );

  /// Denser than [phone]: the two-pane layout gives the conversation less
  /// width, so the avatar column narrows and the flights sit closer together.
  static const MessageRowTokens desktop = MessageRowTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s20),
    avatarSize: S.s32,
    avatarGap: S.s8,
    avatarBottomNudge: S.s4,
    bubbleTextInset: S.s12,
    senderNameGap: S.s4,
    groupGap: S.s12,
  );

  static MessageRowTokens get current => DeviceType.isPhone ? phone : desktop;
}
