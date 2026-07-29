// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:air/ds/foundations/dimensions.dart';
import 'package:air/ds/patterns/context_menu/context_menu_item.dart';
import 'package:air/ds/patterns/context_menu/context_menu_surface.dart';

import 'package:air/features/emoji/emoji_repository.dart';
import 'package:air/features/message_list/message_reactions.dart';

const double _mobileActionRowHeight = 56.0;

class MessageAction {
  const MessageAction({
    required this.label,
    this.leading,
    required this.onSelected,
    this.isDestructive = false,
    this.insertSeparatorBefore = false,
  });

  final String label;
  final Widget? leading;
  final VoidCallback onSelected;
  final bool isDestructive;
  final bool insertSeparatorBefore;
}

Future<void> showMobileMessageActions({
  required BuildContext context,
  required Rect anchorRect,
  required List<MessageAction> actions,
  required Widget messageContent,
  required bool alignEnd,
  EmojiSkinVariation reactionSkinTone = EmojiSkinVariation.none,
  void Function(String emoji)? onReact,
  VoidCallback? onReactMore,
}) {
  // Drop taps that land during the closing transition.
  var consumed = false;
  // Deliver a picked reaction only after the exit transition settled.
  // Mutating the message earlier would relayout the list while the bubble
  // copy flies back and make it land off target.
  String? pickedEmoji;
  var exitListenerAttached = false;
  return showGeneralDialog(
    context: context,
    barrierDismissible: true,
    barrierColor: Colors.transparent,
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    transitionDuration: const Duration(milliseconds: 250),
    pageBuilder: (context, animation, secondaryAnimation) =>
        const SizedBox.shrink(),
    transitionBuilder: (dialogContext, animation, secondaryAnimation, child) {
      bool dismiss() {
        if (consumed) return false;
        consumed = true;
        Navigator.of(dialogContext).pop();
        return true;
      }

      if (!exitListenerAttached) {
        exitListenerAttached = true;
        animation.addStatusListener((status) {
          if (status == AnimationStatus.dismissed && pickedEmoji != null) {
            onReact?.call(pickedEmoji!);
          }
        });
      }

      final curvedAnimation = CurvedAnimation(
        parent: animation,
        curve: Curves.easeOutCubic,
        reverseCurve: Curves.easeInCubic,
      );
      final overlayView = _MobileMessageActionView(
        animation: curvedAnimation,
        anchorRect: anchorRect,
        actions: actions,
        messageContent: messageContent,
        alignEnd: alignEnd,
        reactionSkinTone: reactionSkinTone,
        // Delivered by the exit listener above once the overlay is gone.
        onReact: onReact == null ? null : (emoji) => pickedEmoji = emoji,
        onReactMore: onReactMore,
        onDismiss: dismiss,
      );
      final defaultTextStyle = DefaultTextStyle.of(context);
      return DefaultTextStyle(
        style: defaultTextStyle.style,
        textAlign: defaultTextStyle.textAlign,
        softWrap: defaultTextStyle.softWrap,
        overflow: defaultTextStyle.overflow,
        maxLines: defaultTextStyle.maxLines,
        child: overlayView,
      );
    },
  );
}

class _MobileMessageActionView extends StatelessWidget {
  const _MobileMessageActionView({
    required this.animation,
    required this.anchorRect,
    required this.actions,
    required this.messageContent,
    required this.alignEnd,
    required this.reactionSkinTone,
    required this.onReact,
    required this.onReactMore,
    required this.onDismiss,
  });

  final Animation<double> animation;
  final Rect anchorRect;
  final List<MessageAction> actions;
  final Widget messageContent;
  final bool alignEnd;
  final EmojiSkinVariation reactionSkinTone;
  final void Function(String emoji)? onReact;
  final VoidCallback? onReactMore;

  /// Closes the overlay, returning false when it is already closing so that
  /// callers drop taps that land during the closing transition.
  final bool Function() onDismiss;

  @override
  Widget build(BuildContext context) {
    // Layout inputs derived from the current overlay and safe areas.
    final mediaQuery = MediaQuery.of(context);
    final size = mediaQuery.size;
    final safeTop = mediaQuery.padding.top + Spacing.px24;
    final safeBottom = mediaQuery.padding.bottom + Spacing.px24;
    const gap = Spacing.px32;
    final messageHeight = anchorRect.height;
    final messageWidth = anchorRect.width;

    // Height of the action sheet that may appear below the bubble.
    final double sheetHeight = actions.isEmpty
        ? 0.0
        : actions.length * _mobileActionRowHeight;

    // Downscale bubble if it cannot fit into the available viewport space and
    // reserve space for the action sheet when present.
    final double availableHeight = size.height - safeTop - safeBottom;
    double scale = 1.0;
    if (availableHeight > 0 && messageHeight > availableHeight) {
      scale = availableHeight / messageHeight;
    }
    if (sheetHeight > 0) {
      final double availableWithSheet = availableHeight - (sheetHeight + gap);
      if (availableWithSheet > 0 &&
          messageHeight * scale > availableWithSheet) {
        scale = availableWithSheet / messageHeight;
      }
    }
    scale = scale.clamp(0.0, 1.0);

    final double scaledMessageHeight = messageHeight * scale;

    // Keep bubble top within visible bounds after scaling.
    final double unclampedMaxTop =
        size.height - safeBottom - scaledMessageHeight;
    final double maxTop = unclampedMaxTop < safeTop ? safeTop : unclampedMaxTop;

    final double startTop = anchorRect.top;
    double clampedStartTop = anchorRect.top.clamp(safeTop, maxTop);

    // Nudge the bubble up if needed so that the sheet fits below it.
    double targetTop = clampedStartTop;
    double finalSheetTop = targetTop + scaledMessageHeight + gap;

    if (sheetHeight > 0) {
      double availableBelow =
          size.height - safeBottom - (targetTop + scaledMessageHeight);
      final double required = sheetHeight + gap;

      // If the sheet would overflow below, shift the bubble up.
      if (availableBelow < required) {
        final double deficit = required - availableBelow;
        targetTop = (targetTop - deficit).clamp(safeTop, maxTop);
        availableBelow =
            size.height - safeBottom - (targetTop + scaledMessageHeight);
      }

      finalSheetTop = targetTop + scaledMessageHeight + gap;
      final double sheetBottom = finalSheetTop + sheetHeight;

      if (sheetBottom > size.height - safeBottom) {
        finalSheetTop = size.height - safeBottom - sheetHeight;
        final double newTargetTop = finalSheetTop - gap - scaledMessageHeight;
        targetTop = newTargetTop.clamp(safeTop, maxTop);
        finalSheetTop = targetTop + scaledMessageHeight + gap;
        if (finalSheetTop + sheetHeight > size.height - safeBottom) {
          finalSheetTop = size.height - safeBottom - sheetHeight;
        }
      }
    }

    // Keep horizontal alignment the same as the original bubble.
    final double startLeft = alignEnd
        ? anchorRect.right - messageWidth
        : anchorRect.left;

    // Action sheet starts below the original (unscaled) bubble.
    final double startSheetTop = anchorRect.bottom + gap;
    final double minSheetTop = safeTop;
    final double maxSheetTop =
        size.height - safeBottom - sheetHeight < minSheetTop
        ? minSheetTop
        : size.height - safeBottom - sheetHeight;
    final double clampedFinalSheetTop = finalSheetTop.clamp(
      minSheetTop,
      maxSheetTop,
    );

    // Animate bubble position/scale and the sheet placement together.
    return AnimatedBuilder(
      animation: animation,
      child: IgnorePointer(ignoring: true, child: messageContent),
      builder: (context, child) {
        // Interpolate bubble position while morphing into the scaled state.
        final eased = animation.value;
        final double animatedScale = lerpDouble(1.0, scale, eased)!;
        final double backgroundOpacity = (eased * 0.65).clamp(0.0, 0.65);
        final double blurSigma = lerpDouble(0.0, 16.0, eased)!;
        final double top = lerpDouble(startTop, targetTop, eased)!;
        final double left = startLeft;
        final double width = messageWidth;

        // Slide the sheet from beneath the original bubble to its target.
        final double sheetTop = lerpDouble(
          startSheetTop,
          clampedFinalSheetTop,
          eased,
        )!;

        return Stack(
          children: [
            GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: onDismiss,
              child: BackdropFilter(
                filter: ImageFilter.blur(sigmaX: blurSigma, sigmaY: blurSigma),
                child: Container(
                  color: Colors.black.withValues(alpha: backgroundOpacity),
                ),
              ),
            ),
            Positioned(
              left: left,
              top: top,
              width: width,
              child: Transform.scale(
                scale: animatedScale,
                alignment: alignEnd ? Alignment.topRight : Alignment.topLeft,
                child: child!,
              ),
            ),
            if (onReact != null)
              Positioned(
                left: alignEnd ? null : left,
                right: alignEnd ? (size.width - (left + width)) : null,
                top: (top - quickReactionMenuGap - quickReactionBarHeight)
                    .clamp(safeTop, size.height),
                child: FadeTransition(
                  opacity: animation,
                  child: QuickReactionBar(
                    skinTone: reactionSkinTone,
                    showShadow: false,
                    onReact: (emoji) {
                      if (onDismiss()) onReact!(emoji);
                    },
                    onMore: () {
                      if (onDismiss()) onReactMore?.call();
                    },
                  ),
                ),
              ),
            if (sheetHeight > 0)
              Positioned(
                left: alignEnd ? null : Spacing.px24,
                right: alignEnd ? Spacing.px24 : null,
                top: sheetTop,
                child: _MobileContextMenu(
                  animation: animation,
                  actions: actions,
                  alignEnd: alignEnd,
                  onDismiss: onDismiss,
                ),
              ),
          ],
        );
      },
    );
  }
}

class _MobileContextMenu extends StatelessWidget {
  const _MobileContextMenu({
    required this.animation,
    required this.actions,
    required this.alignEnd,
    required this.onDismiss,
  });

  final Animation<double> animation;
  final List<MessageAction> actions;
  final bool alignEnd;
  final bool Function() onDismiss;

  @override
  Widget build(BuildContext context) {
    final menuItems = <ContextMenuEntry>[];
    for (final action in actions) {
      if (action.insertSeparatorBefore) {
        menuItems.add(const ContextMenuSeparator());
      }
      menuItems.add(
        ContextMenuItem(
          label: action.label,
          leading: action.leading,
          isDestructive: action.isDestructive,
          onPressed: () {
            if (onDismiss()) action.onSelected();
          },
        ),
      );
    }

    final slideAnimation = animation.drive(
      Tween<Offset>(begin: const Offset(0, 0.12), end: Offset.zero),
    );

    return FadeTransition(
      opacity: animation,
      child: SlideTransition(
        position: slideAnimation,
        child: Align(
          alignment: alignEnd ? Alignment.centerRight : Alignment.centerLeft,
          child: IntrinsicWidth(
            child: ContextMenuSurface(menuItems: menuItems, onHide: onDismiss),
          ),
        ),
      ),
    );
  }
}
