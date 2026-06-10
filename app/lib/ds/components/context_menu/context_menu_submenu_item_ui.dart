// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'dart:math' as math;

import 'package:air/ds/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ds/components/context_menu/context_menu_ui.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

class ContextMenuSubmenuItem extends ContextMenuEntry {
  const ContextMenuSubmenuItem({
    super.key,
    required this.label,
    this.leading,
    this.leadingIcon,
    this.reserveLeadingSpace = false,
    required this.subItems,
    this.onSubItemSelected,
    this.parentMenuKey,
  });

  final String label;
  final Widget? leading;
  final IconData? leadingIcon;
  final bool reserveLeadingSpace;
  final List<ContextMenuItem> subItems;
  final VoidCallback? onSubItemSelected;
  // Key of the parent menu container — used to align the submenu's top edge.
  final GlobalKey? parentMenuKey;

  static const double chevronSize = 16.0;

  bool get hasLeading => leading != null || leadingIcon != null;

  ContextMenuSubmenuItem copyWith({
    bool? reserveLeadingSpace,
    VoidCallback? onSubItemSelected,
    GlobalKey? parentMenuKey,
  }) {
    return ContextMenuSubmenuItem(
      key: key,
      label: label,
      leading: leading,
      leadingIcon: leadingIcon,
      reserveLeadingSpace: reserveLeadingSpace ?? this.reserveLeadingSpace,
      subItems: subItems,
      onSubItemSelected: onSubItemSelected ?? this.onSubItemSelected,
      parentMenuKey: parentMenuKey ?? this.parentMenuKey,
    );
  }

  @override
  Widget build(BuildContext context) {
    return _ContextMenuSubmenuItemWidget(item: this);
  }
}

class _ContextMenuSubmenuItemWidget extends StatefulWidget {
  const _ContextMenuSubmenuItemWidget({required this.item});

  final ContextMenuSubmenuItem item;

  @override
  State<_ContextMenuSubmenuItemWidget> createState() =>
      _ContextMenuSubmenuItemWidgetState();
}

class _ContextMenuSubmenuItemWidgetState
    extends State<_ContextMenuSubmenuItemWidget> {
  final _controller = OverlayPortalController();
  final _layerLink = LayerLink();
  final _targetKey = GlobalKey();
  Size? _submenuSize;

  void _handleSubmenuSize(Size size) {
    if (_submenuSize == size || !mounted) return;
    setState(() => _submenuSize = size);
  }

  void _toggleSubmenu() {
    if (_controller.isShowing) {
      _controller.hide();
    } else {
      _controller.show();
    }
  }

  @override
  Widget build(BuildContext context) {
    final item = widget.item;
    final colors = CustomColorScheme.of(context);

    Widget? leadingWidget;
    if (item.leading != null) {
      leadingWidget = item.leading;
    } else if (item.leadingIcon != null) {
      leadingWidget = Icon(
        item.leadingIcon,
        size: ContextMenuItem.defaultLeadingWidth,
      );
    }

    return OverlayPortal(
      controller: _controller,
      overlayChildBuilder: _buildSubmenu,
      child: CompositedTransformTarget(
        key: _targetKey,
        link: _layerLink,
        child: TextButton(
          onPressed: _toggleSubmenu,
          style: TextButton.styleFrom(
            shape: const RoundedRectangleBorder(
              borderRadius: BorderRadius.zero,
            ),
            foregroundColor: colors.text.primary,
            padding: const EdgeInsets.symmetric(vertical: Spacing.px4),
            alignment: Alignment.centerLeft,
            splashFactory: !Platform.isAndroid ? NoSplash.splashFactory : null,
            overlayColor: Colors.transparent,
          ),
          child: Row(
            mainAxisSize: MainAxisSize.max,
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              if (item.reserveLeadingSpace) ...[
                SizedBox(
                  width: ContextMenuItem.defaultLeadingWidth,
                  child: leadingWidget,
                ),
                const SizedBox(width: Spacing.px8),
              ] else if (leadingWidget != null) ...[
                leadingWidget,
                const SizedBox(width: Spacing.px8),
              ],
              Expanded(
                child: Text(
                  item.label,
                  style: TextStyle(fontSize: LabelFontSize.base.size),
                  maxLines: 1,
                  softWrap: false,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              const SizedBox(width: Spacing.px8),
              AppIcon.chevronRight(
                size: ContextMenuSubmenuItem.chevronSize,
                color: colors.text.secondary,
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildSubmenu(BuildContext overlayContext) {
    final item = widget.item;
    final overlayState = Overlay.of(overlayContext);
    final overlayBox = overlayState.context.findRenderObject() as RenderBox?;
    final overlaySize = overlayBox?.size ?? MediaQuery.of(overlayContext).size;

    final submenuWidth = _measureSubmenuWidth(
      overlayContext,
      overlaySize.width,
    );

    double dx = 0;
    double dy = 0;
    if (overlayBox != null) {
      final targetBox =
          _targetKey.currentContext?.findRenderObject() as RenderBox?;
      final parentMenuBox =
          item.parentMenuKey?.currentContext?.findRenderObject() as RenderBox?;

      if (targetBox != null) {
        final itemOffset = targetBox.localToGlobal(
          Offset.zero,
          ancestor: overlayBox,
        );
        final itemRect = itemOffset & targetBox.size;

        // X: use the parent menu container's edge + gap, falling back to the
        // trigger item edge if the parent key isn't available yet.
        final Rect anchorRect;
        if (parentMenuBox != null) {
          final parentOffset = parentMenuBox.localToGlobal(
            Offset.zero,
            ancestor: overlayBox,
          );
          anchorRect = parentOffset & parentMenuBox.size;
        } else {
          anchorRect = itemRect;
        }
        const gap = Spacing.px4;
        final rightSpace = overlaySize.width - anchorRect.right;
        dx = rightSpace >= submenuWidth + gap
            ? anchorRect.right + gap
            : anchorRect.left - submenuWidth - gap;
        dx = dx.clamp(0.0, math.max(0.0, overlaySize.width - submenuWidth));

        // Y: align with the parent menu container top, falling back to the
        // trigger item top if the parent key isn't available yet.
        final rawDy = parentMenuBox != null
            ? parentMenuBox.localToGlobal(Offset.zero, ancestor: overlayBox).dy
            : itemRect.top;
        final menuH = _submenuSize?.height ?? 0.0;
        dy = rawDy.clamp(0.0, math.max(0.0, overlaySize.height - menuH));
      }
    }

    final wrappedSubItems = item.subItems.map((subItem) {
      return subItem.copyWith(
        onPressed: () {
          _controller.hide();
          item.onSubItemSelected?.call();
          subItem.onPressed();
        },
      );
    }).toList();

    return Stack(
      children: [
        Positioned.fill(
          child: GestureDetector(
            behavior: HitTestBehavior.translucent,
            onTap: _controller.hide,
          ),
        ),
        Positioned(
          left: dx,
          top: dy,
          width: submenuWidth,
          child: _TrackSize(
            onChange: _handleSubmenuSize,
            child: ContextMenuUi(
              menuItems: wrappedSubItems,
              onHide: _controller.hide,
            ),
          ),
        ),
      ],
    );
  }

  double _measureSubmenuWidth(BuildContext context, double maxWidth) {
    final textStyle = TextStyle(fontSize: LabelFontSize.base.size);
    final textScaler = MediaQuery.textScalerOf(context);
    final textDirection = Directionality.of(context);
    final items = widget.item.subItems;
    if (items.isEmpty) return 0.0;

    final hasAnyLeading = items.any(
      (item) => item.hasLeading || item.reserveLeadingSpace,
    );
    final leadingWidth = hasAnyLeading
        ? ContextMenuItem.defaultLeadingWidth + Spacing.px8
        : 0.0;
    final trailingIconSize = IconTheme.of(context).size ?? 24.0;

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
        itemWidth += trailingIconSize + Spacing.px8;
      }
      if (itemWidth > widestItem) widestItem = itemWidth;
    }

    final paddedWidth = widestItem + Spacing.px16 * 2;
    if (maxWidth <= 0) return paddedWidth;
    return paddedWidth.clamp(0.0, maxWidth);
  }
}

class _TrackSize extends SingleChildRenderObjectWidget {
  const _TrackSize({required this.onChange, super.child});

  final ValueChanged<Size> onChange;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderTrackSize(onChange);

  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderTrackSize renderObject,
  ) {
    renderObject.onChange = onChange;
  }
}

class _RenderTrackSize extends RenderProxyBox {
  _RenderTrackSize(this.onChange);

  ValueChanged<Size> onChange;
  Size? _lastSize;

  @override
  void performLayout() {
    super.performLayout();
    final newSize = child?.size ?? Size.zero;
    if (newSize == _lastSize) return;
    _lastSize = newSize;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      onChange(newSize);
    });
  }
}
