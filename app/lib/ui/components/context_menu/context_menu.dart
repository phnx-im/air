// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:air/theme/spacings.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/components/context_menu/context_menu_ui.dart';
import 'package:air/ui/typography/font_size.dart';

enum ContextMenuDirection { left, right }

class _ContextMenuCoordinator {
  static OverlayPortalController? _activeController;

  static void register(OverlayPortalController controller) {
    if (_activeController == controller) {
      return;
    }
    if (_activeController?.isShowing ?? false) {
      _activeController!.hide();
    }
    _activeController = controller;
  }

  static void release(OverlayPortalController controller) {
    if (_activeController == controller) {
      _activeController = null;
    }
  }

  static void hideActive() {
    if (_activeController?.isShowing ?? false) {
      _activeController!.hide();
    }
    _activeController = null;
  }
}

class ContextMenu extends StatefulWidget {
  const ContextMenu({
    super.key,
    required this.direction,
    this.offset = Offset.zero,
    required this.controller,
    required this.menuItems,
    this.child,
    this.cursorPosition,
  });

  final ContextMenuDirection direction;
  final Offset offset;
  final OverlayPortalController controller;
  final List<ContextMenuEntry> menuItems;
  final Widget? child;
  final ValueListenable<Offset?>? cursorPosition;

  static void closeActiveMenu() {
    _ContextMenuCoordinator.hideActive();
  }

  @override
  State<ContextMenu> createState() => _ContextMenuState();
}

class _CursorMenuLayoutDelegate extends SingleChildLayoutDelegate {
  const _CursorMenuLayoutDelegate({
    required this.cursorPosition,
    required this.offset,
    required this.safeArea,
  });

  final Offset cursorPosition;
  final Offset offset;
  final EdgeInsets safeArea;

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final bottomRight = Offset(
      cursorPosition.dx + offset.dx,
      cursorPosition.dy + Spacings.xs + offset.dy,
    );

    double dx = bottomRight.dx;
    double dy = bottomRight.dy;

    final rightBoundary = size.width - safeArea.right;
    final bottomBoundary = size.height - safeArea.bottom;

    if (dx + childSize.width > rightBoundary) {
      dx = cursorPosition.dx - childSize.width - offset.dx;
    }

    if (dy + childSize.height > bottomBoundary) {
      dy = cursorPosition.dy - childSize.height - offset.dy - Spacings.xs;
    }

    final minX = safeArea.left;
    final minY = safeArea.top;
    final maxX = (size.width - safeArea.right - childSize.width)
        .clamp(minX, size.width)
        .toDouble();
    final maxY = (size.height - safeArea.bottom - childSize.height)
        .clamp(minY, size.height)
        .toDouble();

    dx = dx.clamp(minX, maxX).toDouble();
    dy = dy.clamp(minY, maxY).toDouble();

    return Offset(dx, dy);
  }

  @override
  bool shouldRelayout(_CursorMenuLayoutDelegate oldDelegate) =>
      oldDelegate.cursorPosition != cursorPosition ||
      oldDelegate.offset != offset ||
      oldDelegate.safeArea != safeArea;
}

class _ContextMenuState extends State<ContextMenu> {
  final LayerLink _layerLink = LayerLink();
  final GlobalKey _targetKey = GlobalKey();
  ValueListenable<Offset?>? _attachedCursorPosition;
  VoidCallback? _cursorPositionListener;
  Size? _menuSize;

  Alignment get _targetAnchor {
    switch (widget.direction) {
      case ContextMenuDirection.left:
        return Alignment.bottomRight;
      case ContextMenuDirection.right:
        return Alignment.bottomLeft;
    }
  }

  Alignment get _followerAnchor {
    switch (widget.direction) {
      case ContextMenuDirection.left:
        return Alignment.topRight;
      case ContextMenuDirection.right:
        return Alignment.topLeft;
    }
  }

  Offset get _followerOffset {
    switch (widget.direction) {
      case ContextMenuDirection.left:
        return Offset(-widget.offset.dx, Spacings.xs + widget.offset.dy);
      case ContextMenuDirection.right:
        return Offset(widget.offset.dx, Spacings.xxs + widget.offset.dy);
    }
  }

  @override
  void initState() {
    super.initState();
    _attachCursorListener();
  }

  @override
  void didUpdateWidget(ContextMenu oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.cursorPosition != widget.cursorPosition) {
      _detachCursorListener();
      _attachCursorListener();
    }
  }

  @override
  void dispose() {
    _detachCursorListener();
    _ContextMenuCoordinator.release(widget.controller);
    super.dispose();
  }

  void _attachCursorListener() {
    final cursorPosition = widget.cursorPosition;
    if (cursorPosition == null || _attachedCursorPosition == cursorPosition) {
      return;
    }

    _cursorPositionListener ??= () {
      if (mounted) {
        setState(() {});
      }
    };

    cursorPosition.addListener(_cursorPositionListener!);
    _attachedCursorPosition = cursorPosition;
  }

  void _detachCursorListener() {
    final cursorPosition = _attachedCursorPosition;
    if (cursorPosition == null || _cursorPositionListener == null) {
      return;
    }

    cursorPosition.removeListener(_cursorPositionListener!);
    _attachedCursorPosition = null;
  }

  void _handleMenuSize(Size size) {
    if (_menuSize == size) {
      return;
    }
    if (!mounted) {
      return;
    }
    setState(() {
      _menuSize = size;
    });
  }

  Offset _anchoredFollowerOffset({
    required Rect targetRect,
    required Size overlaySize,
    required EdgeInsets safeArea,
    required double menuWidth,
    required double? maxHeight,
  }) {
    final verticalGap = widget.direction == ContextMenuDirection.left
        ? Spacings.xs + widget.offset.dy
        : Spacings.xxs + widget.offset.dy;
    final bottomBoundary = overlaySize.height - safeArea.bottom;
    final topBoundary = safeArea.top;
    final spaceBelow = bottomBoundary - targetRect.bottom - verticalGap;
    final spaceAbove = targetRect.top - topBoundary - verticalGap;
    final menuHeight = _menuSize?.height ?? maxHeight ?? 0.0;
    final openBelow = _menuSize == null
        ? spaceBelow >= spaceAbove
        : spaceBelow >= menuHeight || spaceBelow >= spaceAbove;

    final anchorX = widget.direction == ContextMenuDirection.left
        ? targetRect.right
        : targetRect.left;

    double dx = widget.direction == ContextMenuDirection.left
        ? anchorX - menuWidth - widget.offset.dx
        : anchorX + widget.offset.dx;
    double dy = openBelow
        ? targetRect.bottom + verticalGap
        : targetRect.top - menuHeight - verticalGap;

    final minX = safeArea.left;
    final minY = safeArea.top;
    final maxX = math.max(minX, overlaySize.width - safeArea.right - menuWidth);
    final maxY = math.max(
      minY,
      overlaySize.height - safeArea.bottom - menuHeight,
    );
    dx = dx.clamp(minX, maxX).toDouble();
    dy = dy.clamp(minY, maxY).toDouble();

    return Offset(dx - targetRect.left, dy - targetRect.top);
  }

  @override
  Widget build(BuildContext context) {
    // Wrap menu items to hide the menu on selection, leave separators untouched.
    final updatedMenuItems = <ContextMenuEntry>[];
    for (final entry in widget.menuItems) {
      if (entry is ContextMenuItem) {
        updatedMenuItems.add(
          entry.copyWith(
            onPressed: () {
              widget.controller.hide();
              _ContextMenuCoordinator.release(widget.controller);
              entry.onPressed();
            },
          ),
        );
      } else {
        updatedMenuItems.add(entry);
      }
    }

    return OverlayPortal(
      controller: widget.controller,
      child: CompositedTransformTarget(
        key: _targetKey,
        link: _layerLink,
        child: widget.child ?? const SizedBox.shrink(),
      ),

      overlayChildBuilder: (BuildContext context) {
        _ContextMenuCoordinator.register(widget.controller);

        final overlayState = Overlay.of(context);
        final overlayBox =
            overlayState.context.findRenderObject() as RenderBox?;
        final mediaQuery = MediaQuery.of(context);
        final overlaySize = overlayBox?.size ?? mediaQuery.size;
        // Prefer system view data so MediaQuery overrides don't remove insets.
        final viewData = MediaQueryData.fromView(
          WidgetsBinding.instance.platformDispatcher.views.first,
        );
        final rawSafeArea = EdgeInsets.only(
          left: math.max(viewData.viewPadding.left, viewData.viewInsets.left),
          top: math.max(viewData.viewPadding.top, viewData.viewInsets.top),
          right: math.max(
            viewData.viewPadding.right,
            viewData.viewInsets.right,
          ),
          bottom: math.max(
            viewData.viewPadding.bottom,
            viewData.viewInsets.bottom,
          ),
        );
        final safeRect = Rect.fromLTWH(
          rawSafeArea.left,
          rawSafeArea.top,
          (viewData.size.width - rawSafeArea.horizontal).clamp(
            0.0,
            viewData.size.width,
          ),
          (viewData.size.height - rawSafeArea.vertical).clamp(
            0.0,
            viewData.size.height,
          ),
        );
        // Convert the safe bounds into overlay coordinates, accounting for
        // transforms like interface scaling.
        final safeArea = overlayBox == null
            ? rawSafeArea
            : () {
                final localTopLeft = overlayBox.globalToLocal(safeRect.topLeft);
                final localBottomRight = overlayBox.globalToLocal(
                  safeRect.bottomRight,
                );
                final localSafeRect = Rect.fromPoints(
                  localTopLeft,
                  localBottomRight,
                );
                final safeIntersection = localSafeRect.intersect(
                  Offset.zero & overlaySize,
                );
                if (safeIntersection.isEmpty) {
                  return EdgeInsets.zero;
                }
                return EdgeInsets.only(
                  left: safeIntersection.left.clamp(0.0, overlaySize.width),
                  top: safeIntersection.top.clamp(0.0, overlaySize.height),
                  right: (overlaySize.width - safeIntersection.right).clamp(
                    0.0,
                    overlaySize.width,
                  ),
                  bottom: (overlaySize.height - safeIntersection.bottom).clamp(
                    0.0,
                    overlaySize.height,
                  ),
                );
              }();

        Offset? cursorPosition = widget.cursorPosition?.value;
        if (cursorPosition != null && overlayBox != null) {
          cursorPosition = overlayBox.globalToLocal(cursorPosition);
        }

        Rect? targetRect;
        if (cursorPosition == null && overlayBox != null) {
          final targetBox =
              _targetKey.currentContext?.findRenderObject() as RenderBox?;
          if (targetBox != null) {
            final targetOffset = targetBox.localToGlobal(
              Offset.zero,
              ancestor: overlayBox,
            );
            targetRect = targetOffset & targetBox.size;
          }
        }

        double? maxHeight;
        if (overlayBox != null && cursorPosition != null) {
          final topBoundary = safeArea.top;
          final bottomBoundary = overlaySize.height - safeArea.bottom;
          final spaceBelow = bottomBoundary - cursorPosition.dy - Spacings.xs;
          final spaceAbove = cursorPosition.dy - topBoundary - Spacings.xs;
          // Limit the menu to the larger available vertical space.
          maxHeight = spaceBelow > spaceAbove ? spaceBelow : spaceAbove;
        } else if (overlayBox != null && targetRect != null) {
          final baseGap = widget.direction == ContextMenuDirection.left
              ? Spacings.xs
              : Spacings.xxs;
          final topBoundary = safeArea.top;
          final bottomBoundary = overlaySize.height - safeArea.bottom;
          final spaceBelow = bottomBoundary - targetRect.bottom - baseGap;
          final spaceAbove = targetRect.top - topBoundary - baseGap;
          // Limit the menu to the larger available vertical space.
          maxHeight = spaceBelow > spaceAbove ? spaceBelow : spaceAbove;
        }
        if (maxHeight != null) {
          final availableHeight = (overlaySize.height - safeArea.vertical)
              .clamp(0.0, overlaySize.height);
          maxHeight = maxHeight.clamp(0.0, availableHeight);
        }

        final maxMenuWidth = (overlaySize.width - safeArea.horizontal).clamp(
          0.0,
          overlaySize.width,
        );
        // Size to the widest item, then clamp so the menu stays inside the viewport.
        final menuWidth = _measureMenuWidth(context, maxMenuWidth);

        final menuUi = SizedBox(
          width: menuWidth,
          child: _MeasureSize(
            onChange: _handleMenuSize,
            child: ContextMenuUi(
              menuItems: updatedMenuItems,
              maxHeight: maxHeight,
              onHide: () {
                widget.controller.hide();
                _ContextMenuCoordinator.release(widget.controller);
              },
            ),
          ),
        );

        return Focus(
          autofocus: true,
          onKeyEvent: (node, event) {
            if (event.logicalKey == LogicalKeyboardKey.escape &&
                event is KeyDownEvent) {
              widget.controller.hide();
              _ContextMenuCoordinator.release(widget.controller);
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          child: Stack(
            children: [
              Positioned.fill(
                child: GestureDetector(
                  behavior: HitTestBehavior.translucent,
                  onTap: () {
                    widget.controller.hide();
                    _ContextMenuCoordinator.release(widget.controller);
                  },
                ),
              ),
              if (cursorPosition == null && targetRect != null)
                CompositedTransformFollower(
                  link: _layerLink,
                  targetAnchor: Alignment.topLeft,
                  followerAnchor: Alignment.topLeft,
                  offset: _anchoredFollowerOffset(
                    targetRect: targetRect,
                    overlaySize: overlaySize,
                    safeArea: safeArea,
                    menuWidth: menuWidth,
                    maxHeight: maxHeight,
                  ),
                  child: menuUi,
                )
              else if (cursorPosition != null)
                CustomSingleChildLayout(
                  delegate: _CursorMenuLayoutDelegate(
                    cursorPosition: cursorPosition,
                    offset: widget.offset,
                    safeArea: safeArea,
                  ),
                  child: menuUi,
                )
              else
                CompositedTransformFollower(
                  link: _layerLink,
                  targetAnchor: _targetAnchor,
                  followerAnchor: _followerAnchor,
                  offset: _followerOffset,
                  child: menuUi,
                ),
            ],
          ),
        );
      },
    );
  }

  double _measureMenuWidth(BuildContext context, double maxWidth) {
    final textStyle = TextStyle(fontSize: LabelFontSize.base.size);
    final textScaler = MediaQuery.textScalerOf(context);
    final textDirection = Directionality.of(context);
    final trailingIconSize = IconTheme.of(context).size ?? 24.0;
    final items = widget.menuItems.whereType<ContextMenuItem>().toList();
    if (items.isEmpty) {
      return 0.0;
    }
    // Reserve a leading column for all items if any item needs alignment.
    final hasAnyLeading = items.any(
      (item) => item.hasLeading || item.reserveLeadingSpace,
    );
    final leadingWidth = hasAnyLeading
        ? ContextMenuItem.defaultLeadingWidth + Spacings.xxs
        : 0.0;
    var widestItem = 0.0;

    for (final item in items) {
      final textPainter = TextPainter(
        text: TextSpan(text: item.label, style: textStyle),
        maxLines: 1,
        textScaler: textScaler,
        textDirection: textDirection,
      )..layout();
      var itemWidth = textPainter.width + leadingWidth;
      if (item.trailingIcon != null) {
        itemWidth += trailingIconSize + Spacings.xxs;
      }

      if (itemWidth > widestItem) {
        widestItem = itemWidth;
      }
    }

    final paddedWidth = widestItem + Spacings.s * 2;
    if (maxWidth <= 0) {
      return paddedWidth;
    }
    return paddedWidth.clamp(0.0, maxWidth);
  }
}

class _MeasureSize extends SingleChildRenderObjectWidget {
  const _MeasureSize({required this.onChange, super.child});

  final ValueChanged<Size> onChange;

  @override
  RenderObject createRenderObject(BuildContext context) {
    return _RenderMeasureSize(onChange);
  }

  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderMeasureSize renderObject,
  ) {
    renderObject.onChange = onChange;
  }
}

class _RenderMeasureSize extends RenderProxyBox {
  _RenderMeasureSize(this.onChange);

  ValueChanged<Size> onChange;
  Size? _lastSize;

  @override
  void performLayout() {
    super.performLayout();
    final newSize = child?.size ?? Size.zero;
    if (newSize == _lastSize) {
      return;
    }
    _lastSize = newSize;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      onChange(newSize);
    });
  }
}
