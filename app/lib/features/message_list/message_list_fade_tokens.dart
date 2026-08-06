// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Fade values for the message list, per density.
///
/// A feature file rather than a DS one: the message list has no pattern in
/// `ds/patterns/`, and a component-tier file has no business naming it. Same
/// call as `features/navigation/tab_bar_tokens.dart`.
@immutable
class MessageListFadeTokens {
  const MessageListFadeTokens({
    required this.topTail,
    required this.topOpacity,
    required this.bottomHeight,
  });

  /// Ramp below the header bar. The strip starts at the very top of the
  /// viewport, so the caller adds the bar's height and the status bar inset: on
  /// a phone this fade does grow with the notch, unlike the chat list's.
  final double topTail;

  /// Peak alpha at the top edge. Full-screen chrome hides what slides under the
  /// bar, while the two-pane layout only dims it.
  final double topOpacity;

  final double bottomHeight;

  static const double bottomOpacity = Alpha.a80;

  /// Scroll distance over which the bottom strip ramps in. At rest on the
  /// newest message there's nothing below the fold to fade.
  static const double bottomRevealDistance = S.s16;

  /// Clearance between the end of the top fade and the first row at rest.
  static const double contentTopGap = S.s8;

  static const MessageListFadeTokens phone = MessageListFadeTokens(
    topTail: S.s48,
    topOpacity: Alpha.a100,
    bottomHeight: S.s64,
  );

  static const MessageListFadeTokens desktop = MessageListFadeTokens(
    topTail: S.s24,
    topOpacity: Alpha.a80,
    bottomHeight: S.s80,
  );

  static MessageListFadeTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
