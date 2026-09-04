// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/reply_block/reply_block_tokens.dart';
import 'package:flutter/widgets.dart';

/// The message a reply quotes: who sent it and a couple of lines of it, behind
/// a rule down the leading edge.
///
/// Sits above the reply itself inside a bubble, hugs its text, and offers to
/// jump to the original.
///
/// The [preview] is resolved text, not content: a deleted or unreachable
/// original still quotes, with whatever stand-in line the host has for it.
class ReplyBlock extends StatelessWidget {
  const ReplyBlock({
    super.key,
    required this.preview,
    this.senderName,
    required this.fill,
    this.showJumpIndicator = false,
    this.thumbnail,
    this.onTap,
  });

  /// The quoted message, one or two lines of it.
  final String preview;

  /// Who the quoted message is from. Absent where there's nobody to name, as
  /// for a message that was deleted or that the reader never had.
  final String? senderName;

  /// Surface behind the quote.
  final Color fill;

  /// Show the arrow that says the original is still there to jump to. A
  /// [thumbnail] outranks it: a quoted picture trails its own still, which
  /// says the same thing about the original and says more about what it is.
  final bool showJumpIndicator;

  /// A still of the quoted message, where it's a picture.
  final Widget? thumbnail;

  /// Jumps to the quoted message.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final sender = senderName;
    final surface = fill.on(PanelSurface.colorOf(context));

    return PanelSurface(
      color: surface,
      child: Builder(
        builder: (context) {
          final palette = SemanticPalette.of(context);
          final textColorSecondary = PanelSurface.textOf(context).secondary;
          final textColorTertiary = PanelSurface.textOf(context).tertiary;

          final quoted = Container(
            decoration: BoxDecoration(
              border: Border(
                left: BorderSide(
                  color: palette.separator.primary,
                  width: ReplyBlockTokens.accentWidth,
                ),
              ),
            ),
            padding: const EdgeInsets.only(left: ReplyBlockTokens.accentGap),
            child: Column(
              mainAxisSize: .min,
              crossAxisAlignment: .start,
              children: [
                if (sender != null) ...[
                  Text(
                    sender,
                    maxLines: 1,
                    overflow: .ellipsis,
                    style: typeScale.body.xs.style(
                      color: textColorSecondary,
                      weight: Weight.emphasized,
                    ),
                  ),
                  const SizedBox(height: ReplyBlockTokens.senderGap),
                ],
                Text(
                  preview,
                  maxLines: ReplyBlockTokens.previewMaxLines,
                  overflow: .ellipsis,
                  style: typeScale.body.xs.style(color: textColorSecondary),
                ),
              ],
            ),
          );

          final trailing =
              thumbnail ??
              (showJumpIndicator
                  ? Padding(
                      padding: const EdgeInsets.only(
                        top: ReplyBlockTokens.iconTopOffset,
                      ),
                      child: AppIcon.arrowUp(
                        size: ReplyBlockTokens.iconSize,
                        color: textColorTertiary,
                      ),
                    )
                  : null);

          Widget block = quoted;
          if (trailing != null) {
            block = Row(
              crossAxisAlignment: .start,
              children: [
                Expanded(child: quoted),
                const SizedBox(width: ReplyBlockTokens.iconGap),
                trailing,
              ],
            );
          }

          block = Container(
            padding: ReplyBlockTokens.padding,
            decoration: BoxDecoration(
              color: surface,
              borderRadius: BorderRadius.circular(ReplyBlockTokens.radius),
            ),
            child: block,
          );

          if (onTap == null) return block;

          return MouseRegion(
            cursor: SystemMouseCursors.click,
            child: GestureDetector(
              behavior: .opaque,
              onTap: onTap,
              child: block,
            ),
          );
        },
      ),
    );
  }
}
