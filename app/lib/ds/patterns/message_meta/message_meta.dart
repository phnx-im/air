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
/// A pure view. The timestamp arrives formatted and localized, and so does
/// every label, so this pattern only decides the arrangement and the type.
class MessageMeta extends StatelessWidget {
  const MessageMeta({
    super.key,
    required this.timestamp,
    this.isSelf = false,
    this.status,
    this.statusLabel,
    this.editedLabel,
    this.contentOffset,
  });

  /// Formatted, localized time of the message.
  final String timestamp;

  /// Own message. Puts the stamp on the bubble's side and picks the edited
  /// label's color.
  final bool isSelf;

  /// Delivery state, or null to leave the stamp at the timestamp. Null covers
  /// both an incoming message, which has no delivery state to report, and one
  /// whose state is withheld.
  final MessageDeliveryStatus? status;

  /// Label beside the delivery glyph. The states worth spelling out are the
  /// ones the reader may have to act on -- in flight, or failed -- so the rest
  /// pass null and let the glyph speak.
  final String? statusLabel;

  /// Marks the message as edited. Null leaves the marker off.
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
    final delivery = status;
    final edited = editedLabel;

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
              Text(timestamp, style: labelStyle),
              if (delivery != null)
                _DeliveryStamp(
                  status: delivery,
                  label: statusLabel,
                  labelStyle: labelStyle,
                ),
              if (edited != null) ...[
                const _Dot(),
                Text(
                  edited,
                  style: labelStyle.copyWith(
                    color: isSelf
                        ? palette.message.selfEditedLabel
                        : palette.message.otherEditedLabel,
                  ),
                ),
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

/// The delivery glyph and its label, set off from the timestamp by a dot. The
/// separator and the label arrive with the glyph, so a send that lands right
/// away leaves the stamp at the timestamp.
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
          const _Dot(),
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
