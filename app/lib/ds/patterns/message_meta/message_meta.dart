// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/delivery_status/delivery_status.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_meta/message_meta_tokens.dart';
import 'package:flutter/widgets.dart';

export 'package:air/ds/components/delivery_status/delivery_status.dart'
    show MessageDeliveryStatus;

/// The stamp under a message bubble: when it was sent, whether it was edited,
/// and -- on an own message -- how far it got.
///
/// Every part is optional and the ones that are present are separated by dots,
/// so the stamp reads as one line whichever of them the host passes.
///
/// A pure view. The timestamp arrives formatted and localized, and so does
/// every label, so this pattern only decides the arrangement and the type.
class MessageMeta extends StatelessWidget {
  const MessageMeta({
    super.key,
    this.timestamp,
    this.isSelf = false,
    this.status,
    this.statusLabel,
    this.editedLabel,
    this.contentOffset,
  });

  /// Formatted, localized time of the message, or null where the stamp reports
  /// only what the message has to say for itself.
  final String? timestamp;

  /// Own message. Puts the stamp on the bubble's side.
  final bool isSelf;

  /// Delivery state, or null to leave the stamp at the timestamp. Null covers
  /// both an incoming message, which has no delivery state to report, and one
  /// whose state is withheld.
  final MessageDeliveryStatus? status;

  /// Label beside the delivery glyph, naming the state the glyph draws. Null
  /// leaves the glyph to speak on its own.
  final String? statusLabel;

  /// Marks the message as edited, drawn beside a pencil. Null leaves the
  /// marker off.
  final String? editedLabel;

  /// Inset on the bubble's side. Defaults to
  /// [MessageMetaTokens.contentOffset].
  final double? contentOffset;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final labelStyle = typeScale.body.mini.style(
      color: palette.text.tertiary,
      tight: true,
    );
    final offset = contentOffset ?? MessageMetaTokens.contentOffset;
    final time = timestamp;
    final delivery = status;
    final edited = editedLabel;

    // The delivery state closes the line, so the reader finds how far a send
    // got in the same place whether or not the message was edited.
    final parts = <Widget>[
      if (time != null) Text(time, style: labelStyle),
      if (edited != null)
        _EditedStamp(
          label: edited,
          labelStyle: labelStyle,
          ink: palette.text.tertiary,
        ),
      if (delivery != null)
        _DeliveryStamp(
          status: delivery,
          label: statusLabel,
          labelStyle: labelStyle,
        ),
    ];
    if (parts.isEmpty) return const SizedBox.shrink();

    return SelectionContainer.disabled(
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          isSelf ? S.s0 : offset,
          MessageMetaTokens.bubbleGap,
          isSelf ? offset : S.s0,
          MessageMetaTokens.bottomPadding,
        ),
        // The stamp is one line by definition, so a column too narrow to hold
        // it scales it down rather than wrapping or clipping it.
        child: FittedBox(
          fit: BoxFit.scaleDown,
          alignment: isSelf ? Alignment.centerRight : Alignment.centerLeft,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final (index, part) in parts.indexed) ...[
                if (index > 0) const _Dot(),
                part,
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// Separates the parts of the stamp, so timestamp, status, and edited marker
/// read as one line rather than three loose labels.
class _Dot extends StatelessWidget {
  const _Dot();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: MessageMetaTokens.dotGap),
      child: Container(
        width: MessageMetaTokens.dotSize,
        height: MessageMetaTokens.dotSize,
        decoration: BoxDecoration(
          color: SemanticPalette.of(context).text.quaternary,
          shape: BoxShape.circle,
        ),
      ),
    );
  }
}

/// The pencil and the edited marker beside it, set like the delivery stamp so
/// the two read as the same kind of report.
///
/// Takes the stamp's own ink rather than one of its own. The ink is an alpha
/// off the text color, so it settles against whatever the row sits on, and a
/// fixed shade beside it would carry a different weight in either theme.
class _EditedStamp extends StatelessWidget {
  const _EditedStamp({
    required this.label,
    required this.labelStyle,
    required this.ink,
  });

  final String label;
  final TextStyle labelStyle;
  final Color ink;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        AppIcon.pencil(size: MessageMetaTokens.iconSize, color: ink),
        const SizedBox(width: MessageMetaTokens.gap),
        Text(label, style: labelStyle),
      ],
    );
  }
}

/// The delivery glyph and its label. The label arrives with the glyph, so a
/// send that lands right away leaves the stamp at the glyph alone.
class _DeliveryStamp extends StatelessWidget {
  const _DeliveryStamp({
    required this.status,
    required this.label,
    required this.labelStyle,
  });

  final MessageDeliveryStatus status;
  final String? label;
  final TextStyle labelStyle;

  @override
  Widget build(BuildContext context) {
    final label = this.label;

    return DeliveryStatus(
      status: status,
      size: MessageMetaTokens.iconSize,
      builder: (context, glyph, ink) => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          glyph,
          if (label != null) ...[
            const SizedBox(width: MessageMetaTokens.gap),
            Text(label, style: labelStyle.copyWith(color: ink)),
          ],
        ],
      ),
    );
  }
}
