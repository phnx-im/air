// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/panel/panel_surface.dart';
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
    this.enabled = true,
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

  /// A disabled row fades its content and takes no gesture.
  final bool enabled;

  final VoidCallback? onTap;

  /// Reports the global position, so the host can anchor a context menu on it.
  final void Function(Offset position)? onLongPress;

  @override
  Widget build(BuildContext context) {
    final t = tokens;
    final palette = SemanticPalette.of(context);
    final longPress = enabled ? onLongPress : null;
    final active = isActive && t.highlightActive;
    final base = PanelSurface.colorOf(context);
    // The selected row takes the primary fill and its ink, the same way the
    // active cell of the nav rail does.
    final surface = active ? palette.accentBrand.primary : base;
    final ink = active ? palette.accentBrand.onPrimary : null;

    Widget fade(Widget child) => enabled
        ? child
        : Opacity(opacity: StateTokens.disabledContent, child: child);

    // StateLayer owns the tap and its feedback. The outer detector stays for
    // the position-reporting long press and secondary tap, which StateLayer
    // has no equivalent for.
    return GestureDetector(
      behavior: .opaque,
      onLongPressStart: longPress != null
          ? (details) => longPress(details.globalPosition)
          : null,
      onSecondaryTapUp: longPress != null
          ? (details) => longPress(details.globalPosition)
          : null,
      child: StateLayer(
        borderRadius: CornerRadius.px0,
        surface: surface,
        // The active row already carries its fill, so the hover wash would
        // double up on it.
        selected: active,
        enabled: enabled,
        onTap: onTap,
        background: active ? ColoredBox(color: surface) : null,
        // The selected row publishes its own fill and ink. Otherwise the state
        // layer's surface passes through, so a hovered row flips its ink.
        child: Builder(
          builder: (context) => PanelSurface(
            color: active ? surface : PanelSurface.maybeOf(context) ?? base,
            ink: ink ?? PanelSurface.inkOf(context),
            child: _RowContent(
              tokens: t,
              active: active,
              ink: ink,
              hideSeparator: hideSeparator,
              avatar: avatar,
              title: title,
              titleIcon: titleIcon,
              timestamp: timestamp,
              preview: preview,
              trailing: trailing,
              fade: fade,
            ),
          ),
        ),
      ),
    );
  }
}

class _RowContent extends StatelessWidget {
  const _RowContent({
    required this.tokens,
    required this.active,
    required this.ink,
    required this.hideSeparator,
    required this.avatar,
    required this.title,
    required this.titleIcon,
    required this.timestamp,
    required this.preview,
    required this.trailing,
    required this.fade,
  });

  final ChatListItemTokens tokens;
  final bool active;
  final Color? ink;
  final bool hideSeparator;
  final Widget avatar;
  final String title;
  final Widget? titleIcon;
  final Widget? timestamp;
  final Widget? preview;
  final Widget? trailing;
  final Widget Function(Widget child) fade;

  @override
  Widget build(BuildContext context) {
    final t = tokens;
    final palette = SemanticPalette.of(context);
    // The ink of whatever fills the row: the selection's, or the hover's.
    final rowInk = ink ?? PanelSurface.inkOf(context);
    return Padding(
      padding: t.containerPadding,
      child: Row(
        crossAxisAlignment: .start,
        children: [
          fade(
            Padding(padding: ChatListItemTokens.avatarPadding, child: avatar),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: .start,
              children: [
                fade(
                  Column(
                    crossAxisAlignment: .start,
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
                    ],
                  ),
                ),
                Padding(
                  padding: t.separatorPadding,
                  child: Container(
                    height: ChatListItemTokens.separatorWidth,
                    color: hideSeparator || rowInk != null
                        ? const Color(0x00000000)
                        : palette.separator.secondary,
                  ),
                ),
              ],
            ),
          ),
        ],
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
    final text = PanelSurface.textOf(context);
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
                    overflow: .ellipsis,
                    style: typeScale.body.regular.style(
                      color: text.primary,
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
      crossAxisAlignment: .start,
      children: [
        Expanded(
          child: Padding(
            padding: ChatListItemTokens.previewPadding,
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
