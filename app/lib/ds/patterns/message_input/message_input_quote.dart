// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:flutter/widgets.dart';

/// The message a staged reply quotes: who sent it and a couple of lines of
/// it, behind a rule down the leading edge.
class MessageInputQuote extends StatelessWidget {
  const MessageInputQuote({
    super.key,
    required this.preview,
    this.senderName,
    this.thumbnail,
    this.onRemove,
  });

  /// The quoted message, one or two lines of it.
  final String preview;

  /// Who the quoted message is from. Absent where there's nobody to name, as
  /// for a message that was deleted or that the reader never had.
  final String? senderName;

  /// A still of the quoted message, where it's a picture.
  final Widget? thumbnail;

  /// Unstages the reply.
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final sender = senderName;
    final thumbnail = this.thumbnail;
    final onRemove = this.onRemove;

    final quoted = Container(
      decoration: BoxDecoration(
        border: Border(
          left: BorderSide(
            color: palette.separator.primary,
            width: MessageInputQuoteTokens.accentWidth,
          ),
        ),
      ),
      padding: const EdgeInsets.only(left: MessageInputQuoteTokens.accentGap),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (sender != null) ...[
            Text(
              sender,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: typeScale.body.xs.style(
                color: palette.text.secondary,
                weight: Weight.emphasized,
              ),
            ),
            const SizedBox(height: MessageInputQuoteTokens.senderGap),
          ],
          Text(
            preview,
            maxLines: MessageInputQuoteTokens.previewMaxLines,
            overflow: TextOverflow.ellipsis,
            style: typeScale.body.xs.style(color: palette.text.secondary),
          ),
        ],
      ),
    );

    Widget block = quoted;
    if (thumbnail != null) {
      block = Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(child: quoted),
          const SizedBox(width: MessageInputQuoteTokens.thumbGap),
          thumbnail,
        ],
      );
    }

    return Padding(
      padding: const EdgeInsets.only(
        left: MessageInputQuoteTokens.gapSide,
        right: MessageInputQuoteTokens.gapSide,
        top: MessageInputQuoteTokens.gapAbove,
        bottom: MessageInputQuoteTokens.gapBelow,
      ),
      child: Stack(
        // The dismiss button's hit target is wider than its circle, so the ring
        // around it reaches past the block and must not be cropped to it.
        clipBehavior: Clip.none,
        children: [
          Container(
            // The stack hands its children loose constraints, so the block has
            // to claim the width it is given rather than hug the quoted text.
            width: double.infinity,
            padding: MessageInputQuoteTokens.padding,
            decoration: BoxDecoration(
              color: palette.fill.secondary,
              borderRadius: BorderRadius.circular(
                MessageInputQuoteTokens.radius,
              ),
            ),
            child: block,
          ),
          if (onRemove != null)
            Positioned(
              top: MessageInputQuoteTokens.removeOffset,
              right: MessageInputQuoteTokens.removeOffset,
              child: ButtonIcon(
                variant: ButtonIconVariant.solid,
                icon: AppIconType.x,
                size: MessageInputQuoteTokens.removeSize,
                iconSize: MessageInputQuoteTokens.removeIconSize,
                fill: palette.backgroundElevated.primary,
                hitTargetSize: MessageInputQuoteTokens.removeHitTarget,
                onPressed: onRemove,
              ),
            ),
        ],
      ),
    );
  }
}
