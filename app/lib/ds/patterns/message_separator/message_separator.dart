// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_separator/message_separator_tokens.dart';
import 'package:flutter/widgets.dart';

/// What a separator marks, which decides both its weight and its shape.
enum MessageSeparatorVariant {
  /// The day the messages below it were sent. A quiet centered label.
  date,

  /// Where the reader left off. An inverted pill between two rules, loud
  /// enough to find while scrolling past it.
  unread,
}

/// A break between two sections of a conversation.
///
/// The label is the host's: the day names and the unread count are localized
/// and change with the clock, so the separator only paints what it's handed.
class MessageSeparator extends StatelessWidget {
  const MessageSeparator({
    super.key,
    required this.label,
    this.variant = MessageSeparatorVariant.date,
  });

  final String label;
  final MessageSeparatorVariant variant;

  @override
  Widget build(BuildContext context) {
    const tokens = MessageSeparatorTokens.standard;
    final pill = MessageSeparatorPill(label: label, variant: variant);

    if (variant == MessageSeparatorVariant.date) {
      return Padding(
        padding: tokens.datePadding,
        child: Center(child: pill),
      );
    }

    final rule = Expanded(
      child: Container(
        height: tokens.ruleThickness,
        color: SemanticPalette.of(context).separator.primary,
      ),
    );

    return Padding(
      padding: tokens.unreadPadding,
      child: Row(
        children: [
          rule,
          Padding(
            padding: EdgeInsets.symmetric(horizontal: tokens.ruleGap),
            child: pill,
          ),
          rule,
        ],
      ),
    );
  }
}

/// The pill a [MessageSeparator] centers, on its own.
///
/// Separate so the date pill can also float over the conversation while the
/// reader scrolls, tracking the topmost visible message. The floating copy and
/// the inline one have to be the same object or the swap between them shows.
class MessageSeparatorPill extends StatelessWidget {
  const MessageSeparatorPill({
    super.key,
    required this.label,
    this.variant = MessageSeparatorVariant.date,
  });

  final String label;
  final MessageSeparatorVariant variant;

  @override
  Widget build(BuildContext context) {
    const tokens = MessageSeparatorTokens.standard;
    final palette = SemanticPalette.of(context);
    final unread = variant == MessageSeparatorVariant.unread;

    return Container(
      padding: unread ? tokens.unreadPillPadding : tokens.datePillPadding,
      decoration: BoxDecoration(
        // The date pill has to lift off the conversation window in both
        // shells, and in dark those differ (the phone runs on base.primary,
        // the desktop content pane on base.quinary). Quaternary is the first
        // tier above the lighter of the two.
        color: unread
            ? palette.function.neutral.toggleBlack
            : palette.backgroundBase.quaternary,
        borderRadius: BorderRadius.circular(CornerRadius.full),
      ),
      child: Text(
        label,
        style: typeScale.body.s
            .style(
              color: unread
                  ? palette.function.neutral.toggleWhite
                  : palette.text.secondary,
            )
            // Flat line box, so the pill's padding is what sets its height.
            .copyWith(height: 1.0),
      ),
    );
  }
}
