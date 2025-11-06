import 'dart:async';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/widgets.dart'
    show DefaultTextStyle, Directionality, InheritedTheme;
import 'package:air/theme/theme.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/components/context_menu/context_menu_ui.dart';

const double _mobileActionRowHeight = 56.0;

class MessageAction {
  const MessageAction({
    required this.label,
    this.leading,
    required this.onSelected,
  });

  final String label;
  final Widget? leading;
  final VoidCallback onSelected;
}

Future<void> showMobileMessageActions({
  required BuildContext context,
  required BuildContext sourceContext,
  required DefaultTextStyle defaultTextStyle,
  required Rect anchorRect,
  required List<MessageAction> actions,
  required Widget messageContent,
}) {
  return showGeneralDialog(
    context: context,
    barrierDismissible: true,
    barrierColor: Colors.transparent,
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    transitionDuration: const Duration(milliseconds: 250),
    pageBuilder:
        (context, animation, secondaryAnimation) => const SizedBox.shrink(),
    transitionBuilder: (dialogContext, animation, secondaryAnimation, child) {
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
      );
      final mediaQueryData = MediaQuery.of(sourceContext);
      final textDirection = Directionality.of(sourceContext);
      return MediaQuery(
        data: mediaQueryData,
        child: DefaultTextStyle(
          style: defaultTextStyle.style,
          textAlign: defaultTextStyle.textAlign,
          softWrap: defaultTextStyle.softWrap,
          overflow: defaultTextStyle.overflow,
          maxLines: defaultTextStyle.maxLines,
          child: Directionality(
            textDirection: textDirection,
            child: overlayView,
          ),
        ),
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
  });

  final Animation<double> animation;
  final Rect anchorRect;
  final List<MessageAction> actions;
  final Widget messageContent;

  @override
  Widget build(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);
    final size = mediaQuery.size;
    final safeTop = mediaQuery.padding.top + Spacings.m;
    final safeBottom = mediaQuery.padding.bottom + Spacings.m;
    const gap = Spacings.l;
    final messageHeight = anchorRect.height;
    final messageWidth = anchorRect.width;
    final double maxTop = size.height - safeBottom - messageHeight;

    double clampedStartTop = anchorRect.top.clamp(safeTop, maxTop);
    final double startLeft = anchorRect.left;

    final double sheetHeight =
        actions.isEmpty ? 0.0 : actions.length * _mobileActionRowHeight;

    double targetTop = clampedStartTop;
    double finalSheetTop = targetTop + messageHeight + gap;

    if (sheetHeight > 0) {
      double availableBelow =
          size.height - safeBottom - (targetTop + messageHeight);
      final double required = sheetHeight + gap;

      if (availableBelow < required) {
        final double deficit = required - availableBelow;
        targetTop = (targetTop - deficit).clamp(safeTop, maxTop);
        availableBelow = size.height - safeBottom - (targetTop + messageHeight);
      }

      finalSheetTop = targetTop + messageHeight + gap;
      final double sheetBottom = finalSheetTop + sheetHeight;

      if (sheetBottom > size.height - safeBottom) {
        finalSheetTop = size.height - safeBottom - sheetHeight;
        final double newTargetTop = finalSheetTop - gap - messageHeight;
        targetTop = newTargetTop.clamp(safeTop, maxTop);
        finalSheetTop = targetTop + messageHeight + gap;
        if (finalSheetTop + sheetHeight > size.height - safeBottom) {
          finalSheetTop = size.height - safeBottom - sheetHeight;
        }
      }
    }

    final double targetLeft = startLeft;

    final double startSheetTop = anchorRect.bottom + gap;
    final double minSheetTop = safeTop;
    final double maxSheetTop = size.height - safeBottom - sheetHeight;
    final double clampedStartSheetTop = startSheetTop.clamp(
      minSheetTop,
      maxSheetTop,
    );
    final double clampedFinalSheetTop = finalSheetTop.clamp(
      minSheetTop,
      maxSheetTop,
    );

    return AnimatedBuilder(
      animation: animation,
      child: IgnorePointer(ignoring: true, child: messageContent),
      builder: (context, child) {
        final eased = animation.value;
        final double backgroundOpacity = (eased * 0.65).clamp(0.0, 0.65);
        final double blurSigma = lerpDouble(0.0, 16.0, eased)!;

        final double top = lerpDouble(clampedStartTop, targetTop, eased)!;
        final double left = targetLeft;
        final double width = messageWidth;

        final double sheetTop =
            lerpDouble(clampedStartSheetTop, clampedFinalSheetTop, eased)!;

        return Stack(
          children: [
            GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => Navigator.of(context).pop(),
              child: BackdropFilter(
                filter: ImageFilter.blur(sigmaX: blurSigma, sigmaY: blurSigma),
                child: Container(
                  color: Colors.black.withValues(alpha: backgroundOpacity),
                ),
              ),
            ),
            Positioned(left: left, top: top, width: width, child: child!),
            if (sheetHeight > 0)
              Positioned(
                left: Spacings.m,
                right: Spacings.m,
                top: sheetTop,
                child: _MobileContextMenu(
                  animation: animation,
                  actions: actions,
                ),
              ),
          ],
        );
      },
    );
  }
}

class _MobileContextMenu extends StatelessWidget {
  const _MobileContextMenu({required this.animation, required this.actions});

  final Animation<double> animation;
  final List<MessageAction> actions;

  @override
  Widget build(BuildContext context) {
    final items =
        actions
            .map(
              (action) => ContextMenuItem(
                label: action.label,
                leading: action.leading,
                onPressed: () {
                  Navigator.of(context).pop();
                  action.onSelected();
                },
              ),
            )
            .toList();

    final slideAnimation = animation.drive(
      Tween<Offset>(begin: const Offset(0, 0.12), end: Offset.zero),
    );

    return FadeTransition(
      opacity: animation,
      child: SlideTransition(
        position: slideAnimation,
        child: ContextMenuUi(
          menuItems: items,
          onHide: () => Navigator.of(context).pop(),
        ),
      ),
    );
  }
}
