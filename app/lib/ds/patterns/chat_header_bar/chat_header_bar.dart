// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/corner_dot/corner_dot.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/chat_header_bar/chat_header_bar_tokens.dart';
import 'package:flutter/widgets.dart';

/// The header of a chat-depth screen: an avatar plus the name (and an optional
/// subtitle) centered in a tappable pill, with an optional back button leading
/// and a matching trailing spacer.
///
/// A pure view: it renders what it's handed and reports gestures back, so the
/// host owns the chat state.
class ChatHeaderBar extends StatelessWidget {
  const ChatHeaderBar({
    super.key,
    required this.tokens,
    required this.name,
    this.subtitle,
    this.avatar,
    this.onTap,
    this.onLongPress,
    this.onBack,
    this.backEmphasized = false,
  });

  final ChatHeaderBarTokens tokens;
  final String name;

  /// Second line under the name. Rendered only when non-null, and already
  /// localized: the pattern never resolves copy itself.
  final String? subtitle;

  /// Rendered inside the pill. The host sizes it to
  /// [ChatHeaderBarTokens.avatarSize].
  final Widget? avatar;

  /// Opens the chat's profile. The pill is the tap target, so the profile only
  /// opens from the pill, not from dead bar space.
  final VoidCallback? onTap;

  final VoidCallback? onLongPress;

  /// The back button renders only when non-null.
  final VoidCallback? onBack;

  /// Badges the back button with a corner dot, flagging that what the user is
  /// going back to has moved on since they left it.
  final bool backEmphasized;

  @override
  Widget build(BuildContext context) {
    final t = tokens;
    final palette = SemanticPalette.of(context);
    return SizedBox(
      height: ChatHeaderBarTokens.height,
      child: Padding(
        padding: const EdgeInsets.only(
          left: ChatHeaderBarTokens.paddingLeft,
          right: ChatHeaderBarTokens.paddingRight,
        ),
        child: Row(
          children: [
            SizedBox(
              width: t.slotSize,
              child: Align(
                alignment: Alignment.centerLeft,
                child: onBack != null
                    ? CornerDot(
                        show: backEmphasized,
                        // The pill is tappable too, so the back button and the
                        // pill share one shadow tier and read as the same kind
                        // of floating control.
                        child: ButtonIcon(
                          variant: ButtonIconVariant.solid,
                          icon: AppIconType.arrowLeft,
                          size: t.slotSize,
                          fill: palette.backgroundElevated.primary,
                          shadows: Effect.elevation(Elevation.flat),
                          onPressed: onBack,
                        ),
                      )
                    : const SizedBox.shrink(),
              ),
            ),
            Expanded(
              child: Center(
                child: _TitlePill(
                  tokens: t,
                  name: name,
                  subtitle: subtitle,
                  avatar: avatar,
                  onTap: onTap,
                  onLongPress: onLongPress,
                ),
              ),
            ),
            // Trailing spacer, so the pill stays optically centered.
            SizedBox(width: t.slotSize),
          ],
        ),
      ),
    );
  }
}

/// Avatar plus name inside the tappable pill. The avatar sits inside the pill,
/// inset by its leading padding, so it reads as part of the chip rather than
/// floating beside it.
class _TitlePill extends StatelessWidget {
  const _TitlePill({
    required this.tokens,
    required this.name,
    required this.subtitle,
    required this.avatar,
    required this.onTap,
    required this.onLongPress,
  });

  final ChatHeaderBarTokens tokens;
  final String name;
  final String? subtitle;
  final Widget? avatar;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final avatarWidget = avatar;
    final subtitleText = subtitle;

    return MouseRegion(
      cursor: onTap != null ? SystemMouseCursors.click : MouseCursor.defer,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        onLongPress: onLongPress,
        child: Container(
          constraints: BoxConstraints(minHeight: tokens.pillMinHeight),
          padding: ChatHeaderBarTokens.pillPadding,
          decoration: BoxDecoration(
            color: palette.backgroundElevated.primary,
            borderRadius: BorderRadius.circular(ChatHeaderBarTokens.pillRadius),
            boxShadow: Effect.elevation(Elevation.flat),
          ),
          // mainAxisSize.min so the pill hugs its content instead of expanding
          // to the bar's full width.
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (avatarWidget != null) ...[
                avatarWidget,
                const SizedBox(width: ChatHeaderBarTokens.gap),
              ],
              Flexible(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: avatarWidget != null
                      ? CrossAxisAlignment.start
                      : CrossAxisAlignment.center,
                  children: [
                    Text(
                      name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      // The pill's height comes from the avatar, so the label
                      // drops the body line height and sits tight.
                      style: typeScale.body.regular.style(
                        color: palette.text.primary,
                        tight: true,
                      ),
                    ),
                    if (subtitleText != null)
                      Padding(
                        padding: const EdgeInsets.only(
                          top: ChatHeaderBarTokens.titleGap,
                        ),
                        child: Text(
                          subtitleText,
                          style: typeScale.body.xs.style(
                            color: palette.text.tertiary,
                            tight: true,
                          ),
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
