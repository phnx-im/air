// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the separators that break a conversation into sections.
///
/// Geometry only: colors come from the palette at paint time. One set for
/// both densities, a separator is a line of text either way.
abstract final class MessageSeparatorTokens {
  /// Inset around a date separator. Generous, so a new day reads as a break in
  /// the conversation rather than as another row in it.
  static const EdgeInsets datePadding = EdgeInsets.symmetric(
    horizontal: S.s24,
    vertical: S.s32,
  );

  /// Inset around an unread separator. Tighter than [datePadding]: it marks
  /// where the reader stopped, so it belongs to the messages around it.
  static const EdgeInsets unreadPadding = EdgeInsets.symmetric(
    horizontal: S.s16,
    vertical: S.s16,
  );

  static const EdgeInsets datePillPadding = EdgeInsets.symmetric(
    horizontal: S.s16,
    vertical: S.s4,
  );

  /// Taller than [datePillPadding]. The unread pill is a filled marker rather
  /// than a label, and carries the extra weight to say so.
  static const EdgeInsets unreadPillPadding = EdgeInsets.symmetric(
    horizontal: S.s16,
    vertical: S.s8,
  );

  static const double ruleThickness = StrokeWidth.px0_5;

  /// Gap between the pill and the rules either side of it.
  static const double ruleGap = S.s16;
}
