// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/icons.dart';
import 'package:air/ds/patterns/reminder_banner/reminder_banner_tokens.dart';

/// A static, in-flow reminder card -- a rounded banner with a leading icon,
/// bold title, body text, and an optional dismiss button. This is meant to sit
/// persistently in content, e.g. pinned at the top of a list.
///
/// Pure function of its [tokens]. When [onDismiss] is null the card is
/// non-dismissable (no close button) -- the seam for reminders the user can't
/// dismiss. The [title]/[body]/[type] are host-supplied so `ds/` carries no
/// app copy.
class ReminderBanner extends StatelessWidget {
  const ReminderBanner({
    super.key,
    required this.tokens,
    required this.type,
    required this.title,
    required this.body,
    this.onTap,
    this.onDismiss,
  });

  final ReminderBannerTokens tokens;
  final AppIconType type;
  final String title;
  final String body;

  /// Null => the card is non-tappable.
  final VoidCallback? onTap;

  /// Null => the card renders no dismiss button (non-dismissable).
  final VoidCallback? onDismiss;

  @override
  Widget build(BuildContext context) {
    final t = tokens;
    final dismissable = onDismiss != null;

    final decoration = BoxDecoration(
      color: t.background,
      borderRadius: BorderRadius.circular(t.radius),
    );

    final card = Stack(
      children: [
        Padding(
          padding: t.padding,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              SizedBox(
                width: t.leadingColumnWidth,
                child: Center(
                  child: AppIcon(
                    type: type,
                    size: t.iconSize,
                    color: t.iconColor,
                  ),
                ),
              ),
              SizedBox(width: t.gap),
              Expanded(
                // Keep the text clear of the top-right dismiss button, with
                // [textDismissGap] of breathing space before it.
                child: Padding(
                  padding: EdgeInsets.only(
                    right: dismissable
                        ? t.dismissInset +
                              t.dismissButtonSize +
                              t.textDismissGap -
                              t.padding.right
                        : 0,
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.titleStyle,
                      ),
                      SizedBox(height: t.titleBodyGap),
                      Text(body, style: t.bodyStyle),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
        if (dismissable)
          Positioned(
            top: t.dismissInset,
            right: t.dismissInset,
            child: StateLayer(
              onTap: onDismiss,
              borderRadius: t.dismissButtonSize / 2,
              surface: t.background,
              background: DecoratedBox(
                decoration: BoxDecoration(
                  color: t.dismissBackground,
                  shape: BoxShape.circle,
                ),
              ),
              child: SizedBox(
                width: t.dismissButtonSize,
                height: t.dismissButtonSize,
                child: Center(
                  child: AppIcon(
                    type: .x,
                    size: t.dismissIconSize,
                    color: t.dismissColor,
                  ),
                ),
              ),
            ),
          ),
      ],
    );

    if (onTap == null) {
      return DecoratedBox(decoration: decoration, child: card);
    }

    return StateLayer(
      onTap: onTap,
      borderRadius: t.radius,
      surface: t.background,
      background: DecoratedBox(decoration: decoration),
      child: card,
    );
  }
}
