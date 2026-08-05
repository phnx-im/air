// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/chat_list/chat_list_item_tokens.dart';
import 'package:flutter/widgets.dart';

/// A single conversation row: avatar, then a column of title plus timestamp,
/// preview plus trailing indicator, and the separator to the next row.
///
/// A pure view: it renders the slots it's handed and reports gestures back, so
/// the host owns the chat state and every piece of localized copy.
class ChatListItem extends StatelessWidget {
  const ChatListItem({
    super.key,
    required this.tokens,
    required this.title,
    required this.avatar,
    this.titleIcon,
    this.timestamp,
    this.preview,
    this.trailing,
    this.isActive = false,
    this.hideSeparator = false,
    this.onTap,
    this.onLongPress,
  });

  final ChatListItemTokens tokens;
  final String title;

  /// The host sizes it to [ChatListItemTokens.avatarSize].
  final Widget avatar;

  /// Glyph beside the title, sized by the host to
  /// [ChatListItemTokens.titleIconSize].
  final Widget? titleIcon;

  final Widget? timestamp;
  final Widget? preview;

  /// Unread counter, delivery status, or nothing.
  final Widget? trailing;

  final bool isActive;

  /// Paints the separator transparent instead of dropping it, so the row's
  /// height stays the same either way.
  final bool hideSeparator;

  final VoidCallback? onTap;

  /// Reports the global position, so the host can anchor a context menu on it.
  final void Function(Offset position)? onLongPress;

  @override
  Widget build(BuildContext context) {
    final t = tokens;
    final palette = SemanticPalette.of(context);
    final longPress = onLongPress;
    final active = isActive && t.highlightActive;

    // StateLayer owns the tap and its feedback. The outer detector stays for
    // the position-reporting long press and secondary tap, which StateLayer
    // has no equivalent for.
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onLongPressStart: longPress != null
          ? (details) => longPress(details.globalPosition)
          : null,
      onSecondaryTapUp: longPress != null
          ? (details) => longPress(details.globalPosition)
          : null,
      child: StateLayer(
        borderRadius: CornerRadius.px0,
        surface: active
            ? palette.fill.tertiary
            : palette.backgroundBase.primary,
        // The active row already carries its fill, so the hover wash would
        // double up on it.
        selected: active,
        onTap: onTap,
        background: active ? ColoredBox(color: palette.fill.tertiary) : null,
        child: Padding(
          padding: t.containerPadding,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(padding: t.avatarPadding, child: avatar),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    _TitleRow(
                      tokens: t,
                      title: title,
                      titleIcon: titleIcon,
                      timestamp: timestamp,
                    ),
                    _PreviewRow(
                      tokens: t,
                      preview: preview,
                      trailing: trailing,
                    ),
                    Padding(
                      padding: t.separatorPadding,
                      child: Container(
                        height: ChatListItemTokens.separatorWidth,
                        color: hideSeparator
                            ? const Color(0x00000000)
                            : palette.separator.secondary,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _TitleRow extends StatelessWidget {
  const _TitleRow({
    required this.tokens,
    required this.title,
    required this.titleIcon,
    required this.timestamp,
  });

  final ChatListItemTokens tokens;
  final String title;
  final Widget? titleIcon;
  final Widget? timestamp;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final icon = titleIcon;
    final time = timestamp;

    return Row(
      children: [
        Expanded(
          child: Padding(
            padding: tokens.namePadding,
            child: Row(
              children: [
                // Flexible, so a long title ellipsizes rather than pushing the
                // glyph out of the row.
                Flexible(
                  child: Text(
                    title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: typeScale.body.regular.style(
                      color: palette.text.primary,
                      weight: Weight.emphasized,
                    ),
                  ),
                ),
                if (icon != null)
                  Padding(
                    padding: const EdgeInsets.only(
                      left: ChatListItemTokens.titleIconGap,
                    ),
                    child: icon,
                  ),
              ],
            ),
          ),
        ),
        if (time != null) Padding(padding: tokens.timePadding, child: time),
      ],
    );
  }
}

class _PreviewRow extends StatelessWidget {
  const _PreviewRow({
    required this.tokens,
    required this.preview,
    required this.trailing,
  });

  final ChatListItemTokens tokens;
  final Widget? preview;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final trailingWidget = trailing;
    // We reserve two preview lines as a floor rather than a fixed box, so
    // accessibility text scaling grows the row instead of clipping it. The
    // reserve comes off the preview type token's own line height, never a
    // separate ratio.
    final minHeight = 2 * typeScale.body.s.lineHeightPx;

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(
          child: Padding(
            padding: tokens.previewPadding,
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: minHeight),
              child: Align(
                alignment: AlignmentDirectional.topStart,
                heightFactor: 1,
                child: preview ?? const SizedBox.shrink(),
              ),
            ),
          ),
        ),
        if (trailingWidget != null)
          Padding(padding: tokens.trailingPadding, child: trailingWidget),
      ],
    );
  }
}
