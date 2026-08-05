// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for a chat-list row, per density.
///
/// Geometry only: colors come from the palette at paint time.
///
/// Spacing is a full [EdgeInsets] per element rather than shared gaps, so every
/// side of every element stays independently tunable.
@immutable
class ChatListItemTokens {
  const ChatListItemTokens({
    required this.avatarSize,
    required this.containerPadding,
    required this.avatarPadding,
    required this.namePadding,
    required this.timePadding,
    required this.previewPadding,
    required this.trailingPadding,
    required this.separatorPadding,
    required this.highlightActive,
  });

  final double avatarSize;

  /// Inset around the whole row. The trailing and bottom sides stay at zero on
  /// the phone: the elements carry those insets themselves, so the separator
  /// can run past them to the row's edge.
  final EdgeInsets containerPadding;

  final EdgeInsets avatarPadding;
  final EdgeInsets namePadding;
  final EdgeInsets timePadding;
  final EdgeInsets previewPadding;

  /// Inset around the unread counter or the delivery status.
  final EdgeInsets trailingPadding;

  final EdgeInsets separatorPadding;

  /// Whether a selected row keeps a resting highlight. Only the two-pane layout
  /// holds a selection: on a phone the tapped row navigates away, so there's
  /// never a selected row left on screen to mark.
  final bool highlightActive;

  /// Glyph inline in the preview line.
  static const double previewIconSize = S.s12;
  static const double previewIconGap = S.s4;

  /// Glyph beside the title.
  static const double titleIconSize = S.s16;
  static const double titleIconGap = S.s4;

  static const double separatorWidth = StrokeWidth.px0_5;

  /// Delivery glyph in the trailing gutter. A step above the row's inline
  /// glyphs: it's the gutter's only occupant, and it reads at a glance.
  static const double statusIconSize = S.s16;

  static const ChatListItemTokens phone = ChatListItemTokens(
    avatarSize: S.s48,
    containerPadding: EdgeInsets.fromLTRB(S.s16, S.s16, S.s0, S.s0),
    avatarPadding: EdgeInsets.only(right: S.s12),
    namePadding: EdgeInsets.only(top: S.s2, right: S.s24, bottom: S.s2),
    timePadding: EdgeInsets.only(top: S.s2, right: S.s16, bottom: S.s2),
    previewPadding: EdgeInsets.only(right: S.s8, bottom: S.s12),
    trailingPadding: EdgeInsets.only(right: S.s16, bottom: S.s12),
    separatorPadding: EdgeInsets.only(right: S.s16),
    highlightActive: false,
  );

  static const ChatListItemTokens desktop = ChatListItemTokens(
    avatarSize: S.s40,
    containerPadding: EdgeInsets.fromLTRB(S.s16, S.s12, S.s16, S.s0),
    avatarPadding: EdgeInsets.only(right: S.s12),
    namePadding: EdgeInsets.only(top: S.s2, right: S.s8, bottom: S.s2),
    timePadding: EdgeInsets.only(bottom: S.s2),
    previewPadding: EdgeInsets.only(right: S.s8, bottom: S.s12),
    trailingPadding: EdgeInsets.only(bottom: S.s8),
    separatorPadding: EdgeInsets.zero,
    highlightActive: true,
  );

  /// The two-pane layout is denser and keeps a selection.
  static ChatListItemTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
