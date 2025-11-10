// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/theme/spacings.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/components/context_menu/context_menu_ui.dart';

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
    required this.width,
    required this.controller,
    required this.menuItems,
    this.child,
    this.cursorPosition,
  });

  final ContextMenuDirection direction;
  final Offset offset;
  final double width;
  final OverlayPortalController controller;
  final List<ContextMenuItem> menuItems;
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
  });

  final Offset cursorPosition;
  final Offset offset;

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final bottomRight = Offset(
      cursorPosition.dx + offset.dx,
      cursorPosition.dy + Spacings.xs + offset.dy,
    );

    double dx = bottomRight.dx;
    double dy = bottomRight.dy;

    if (dx + childSize.width > size.width) {
      dx = cursorPosition.dx - childSize.width - offset.dx;
    }

    if (dy + childSize.height > size.height) {
      dy = cursorPosition.dy - childSize.height - offset.dy - Spacings.xs;
    }

    final double maxX =
        (size.width - childSize.width).clamp(0.0, size.width).toDouble();
    final double maxY =
        (size.height - childSize.height).clamp(0.0, size.height).toDouble();

    dx = dx.clamp(0.0, maxX).toDouble();
    dy = dy.clamp(0.0, maxY).toDouble();

    return Offset(dx, dy);
  }

  @override
  bool shouldRelayout(_CursorMenuLayoutDelegate oldDelegate) =>
      oldDelegate.cursorPosition != cursorPosition ||
      oldDelegate.offset != offset;
}

class _ContextMenuState extends State<ContextMenu> {
  final LayerLink _layerLink = LayerLink();
  ValueListenable<Offset?>? _attachedCursorPosition;
  VoidCallback? _cursorPositionListener;

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

  @override
  Widget build(BuildContext context) {
    // Add hide to menu items and store it menu items

    final updatedMenuItems = <ContextMenuItem>[];
    for (final item in widget.menuItems) {
      updatedMenuItems.add(
        item.copyWith(
          onPressed: () {
            widget.controller.hide();
            _ContextMenuCoordinator.release(widget.controller);
            item.onPressed();
          },
        ),
      );
    }

    return OverlayPortal(
      controller: widget.controller,
      child: CompositedTransformTarget(
        link: _layerLink,
        child: widget.child ?? const SizedBox.shrink(),
      ),

      overlayChildBuilder: (BuildContext context) {
        _ContextMenuCoordinator.register(widget.controller);

        Offset? cursorPosition = widget.cursorPosition?.value;
        if (cursorPosition != null) {
          final overlayState = Overlay.of(context);
          final overlayBox =
              overlayState.context.findRenderObject() as RenderBox?;
          if (overlayBox != null) {
            cursorPosition = overlayBox.globalToLocal(cursorPosition);
          }
        }

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
              if (cursorPosition == null)
                CompositedTransformFollower(
                  link: _layerLink,
                  targetAnchor: _targetAnchor,
                  followerAnchor: _followerAnchor,
                  offset: _followerOffset,
                  child: SizedBox(
                    width: widget.width,
                    child: ContextMenuUi(
                      menuItems: updatedMenuItems,
                      onHide: () {
                        widget.controller.hide();
                        _ContextMenuCoordinator.release(widget.controller);
                      },
                    ),
                  ),
                )
              else
                CustomSingleChildLayout(
                  delegate: _CursorMenuLayoutDelegate(
                    cursorPosition: cursorPosition,
                    offset: widget.offset,
                  ),
                  child: SizedBox(
                    width: widget.width,
                    child: ContextMenuUi(
                      menuItems: updatedMenuItems,
                      onHide: () {
                        widget.controller.hide();
                        _ContextMenuCoordinator.release(widget.controller);
                      },
                    ),
                  ),
                ),
            ],
          ),
        );
      },
    );
  }
}
