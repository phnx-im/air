// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_row/message_row_tokens.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// The grid one message sits in: an avatar column, the sender's name above the
/// bubble, and the bubble itself.
///
/// An incoming row hugs the leading edge and keeps a gutter on the far side, an
/// [outgoing] one mirrors both. Everything but the geometry comes from the
/// host: [avatar] is a slot, and the row itself never knows who sent what.
class MessageRow extends StatelessWidget {
  const MessageRow({
    super.key,
    required this.tokens,
    required this.child,
    this.outgoing = false,
    this.avatar,
    this.reserveAvatar = false,
    this.senderName,
    this.onTapSender,
  });

  final MessageRowTokens tokens;

  /// The message itself, usually a bubble. Takes at most
  /// [MessageRowTokens.contentFlex] parts of the row and aligns to the side the
  /// row hugs, so it can shrink to its content.
  final Widget child;

  /// Anchor the row to the trailing edge, for a message the user sent. An
  /// outgoing row has no avatar column: there's only one person it'd show, and
  /// that's the reader themselves.
  final bool outgoing;

  /// The sender's avatar, on the row of a group that shows one. It sits at the
  /// foot of its column, level with the bottom of the bubble beside it, so a
  /// group hands it to the row that closes it. We take it as a widget to keep
  /// the row free of app data, and it owns its own tap: [onTapSender] only
  /// covers the name.
  final Widget? avatar;

  /// Keep the avatar column even when [avatar] is null. Group chats reserve it
  /// on every incoming row, so a group's bubbles all start at the same inset.
  final bool reserveAvatar;

  /// Shown above the bubble, on the row of a group that opens it. Inset to the
  /// bubble's text rather than its edge.
  final String? senderName;

  /// Opens the sender's profile. Without it the name is inert, like in a 1:1
  /// chat where the other person is already on screen.
  final VoidCallback? onTapSender;

  @override
  Widget build(BuildContext context) {
    final avatar = this.avatar;
    final withAvatar = !outgoing && (reserveAvatar || avatar != null);
    final inset = tokens.contentInset(withAvatar: withAvatar);
    final name = senderName;

    return Padding(
      padding: tokens.padding,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: outgoing
            ? CrossAxisAlignment.end
            : CrossAxisAlignment.start,
        children: [
          if (name != null)
            Padding(
              padding: EdgeInsets.only(
                left: inset,
                bottom: MessageRowTokens.senderNameGap,
              ),
              child: _SenderName(name: name, onTap: onTapSender),
            ),
          Row(
            // The message reports the bubble's bottom edge as its baseline,
            // see [MessageBand]. Aligning on it rather than on the row's end
            // keeps the avatar level with the bubble however much the message
            // hangs below it (reaction chips, the stamp under the bubble).
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              if (outgoing) const Spacer(flex: MessageRowTokens.gutterFlex),
              if (withAvatar) ...[
                SizedBox(
                  width: tokens.avatarSize,
                  child: avatar != null ? _BottomBaseline(child: avatar) : null,
                ),
                const SizedBox(width: MessageRowTokens.avatarGap),
              ],
              Expanded(
                flex: MessageRowTokens.contentFlex,
                // Height factor keeps the bubble at its own height: inside a
                // scroll view the row's vertical constraints are unbounded.
                child: Align(
                  alignment: outgoing
                      ? Alignment.centerRight
                      : Alignment.centerLeft,
                  heightFactor: 1,
                  child: child,
                ),
              ),
              if (!outgoing) const Spacer(flex: MessageRowTokens.gutterFlex),
            ],
          ),
        ],
      ),
    );
  }
}

/// Gives the avatar a baseline at its bottom edge, which is what the row
/// aligns with the baseline the message reports at the bubble's bottom edge.
class _BottomBaseline extends SingleChildRenderObjectWidget {
  const _BottomBaseline({required Widget child}) : super(child: child);

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderBottomBaseline();
}

class _RenderBottomBaseline extends RenderProxyBox {
  @override
  double? computeDistanceToActualBaseline(TextBaseline baseline) => size.height;

  @override
  double? computeDryBaseline(
    BoxConstraints constraints,
    TextBaseline baseline,
  ) => child?.getDryLayout(constraints).height ?? constraints.smallest.height;
}

/// The sender's name, kept out of the selection so drag-selecting a
/// conversation copies what people said, not their names.
class _SenderName extends StatelessWidget {
  const _SenderName({required this.name, this.onTap});

  final String name;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final label = SelectionContainer.disabled(
      child: Text(
        name,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: typeScale.body.s.style(
          color: SemanticPalette.of(context).text.primary,
          weight: Weight.emphasized,
        ),
      ),
    );

    if (onTap == null) return label;

    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: label,
      ),
    );
  }
}
