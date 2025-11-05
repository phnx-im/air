// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:air/theme/spacings.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/components/context_menu/context_menu_ui.dart';

enum ContextMenuDirection { left, right }

class ContextMenu extends StatefulWidget {
  const ContextMenu({
    super.key,
    required this.direction,
    this.offset = Offset.zero,
    required this.width,
    required this.controller,
    required this.menuItems,
    this.child,
  });

  final ContextMenuDirection direction;
  final Offset offset;
  final double width;
  final OverlayPortalController controller;
  final List<ContextMenuItem> menuItems;
  final Widget? child;

  @override
  State<ContextMenu> createState() => _ContextMenuState();
}

class _ContextMenuState extends State<ContextMenu> {
  final LayerLink _layerLink = LayerLink();

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
        return Offset(
          -widget.offset.dx,
          Spacings.xs + widget.offset.dy,
        );
      case ContextMenuDirection.right:
        return Offset(
          widget.offset.dx,
          Spacings.xxs + widget.offset.dy,
        );
    }
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
        return Focus(
          autofocus: true,
          onKeyEvent: (node, event) {
            if (event.logicalKey == LogicalKeyboardKey.escape &&
                event is KeyDownEvent) {
              widget.controller.hide();
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          child: Stack(
            children: [
              Positioned.fill(
                child: GestureDetector(
                  behavior: HitTestBehavior.translucent,
                  onTap: () => widget.controller.hide(),
                ),
              ),
              CompositedTransformFollower(
                link: _layerLink,
                targetAnchor: _targetAnchor,
                followerAnchor: _followerAnchor,
                offset: _followerOffset,
                child: SizedBox(
                  width: widget.width,
                  child: ContextMenuUi(
                    menuItems: updatedMenuItems,
                    onHide: widget.controller.hide,
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
