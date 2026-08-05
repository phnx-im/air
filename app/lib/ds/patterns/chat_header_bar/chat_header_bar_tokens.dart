// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the chat-depth header, per density.
///
/// Geometry only: colors come from the palette at paint time, and the
/// surface the header floats over is the host's to supply.
@immutable
class ChatHeaderBarTokens {
  const ChatHeaderBarTokens({
    required this.slotSize,
    required this.avatarSize,
    required this.pillMinHeight,
  });

  /// Width reserved on both sides of the pill, so it stays optically centered
  /// even though only the leading slot is ever filled. The back button renders
  /// in that leading slot.
  final double slotSize;

  /// Avatar diameter inside the pill.
  final double avatarSize;

  /// Floor for the pill's height, the avatar plus [pillPadding] sets it above
  /// this.
  final double pillMinHeight;

  static const double height = S.s56;

  // [paddingLeft] matches the list header's, so the leading button sits at the
  // same x in both bars rather than jumping on the way into a conversation.
  static const double paddingLeft = S.s20;
  static const double paddingRight = S.s16;

  /// Gap between the avatar and the title column.
  static const double gap = S.s12;

  /// Gap between the name and the subtitle.
  static const double titleGap = S.s2;

  /// Inset between the pill's edge and its content. The leading side is tight
  /// (the avatar sits nearly flush), the trailing side carries the text inset.
  static const EdgeInsets pillPadding = EdgeInsets.only(
    left: S.s4,
    top: S.s4,
    bottom: S.s4,
    right: S.s16,
  );

  /// Pixels of scroll over which the pill's fill and shadow ramp in from
  /// transparent. At the top of the conversation nothing sits under the bar, so
  /// there's nothing for the pill to separate from.
  static const double pillRevealDistance = S.s40;

  static const double pillRadius = CornerRadius.full;

  /// Emphasis dot pinned to the back button's top-right corner. Sits flush
  /// with the right edge, where the arrow leaves the most room. Same treatment
  /// as the composer's unread dot, so both read as the same badge.
  static const double backDotSize = S.s8;
  static const double backDotInsetTop = S.s2;
  static const double backDotInsetRight = S.s0;

  // The avatar sits 4px in from the pill's leading, top, and bottom edges.
  // Those insets plus the avatar are what set the pill's height, so it lands on
  // the header button's size: phone 32 + 8 = 40, desktop 24 + 8 = 32.
  static const ChatHeaderBarTokens phone = ChatHeaderBarTokens(
    slotSize: S.s40,
    avatarSize: S.s32,
    pillMinHeight: S.s40,
  );

  static const ChatHeaderBarTokens desktop = ChatHeaderBarTokens(
    slotSize: S.s24,
    avatarSize: S.s24,
    pillMinHeight: S.s32,
  );

  static ChatHeaderBarTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
