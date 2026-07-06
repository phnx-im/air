// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:flutter/material.dart';

abstract class ContextMenuEntry extends StatelessWidget {
  const ContextMenuEntry({super.key});
}

class ContextMenuSeparator extends ContextMenuEntry {
  const ContextMenuSeparator({super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Spacing.px8),
      child: Divider(
        height: 0,
        thickness: 1,
        color: CustomColorScheme.of(context).separator.primary,
      ),
    );
  }
}

class ContextMenuItem extends ContextMenuEntry {
  const ContextMenuItem({
    super.key,
    required this.onPressed,
    required this.label,
    this.leadingIcon,
    this.leading,
    this.trailingIcon,
    this.reserveLeadingSpace = false,
    this.isDestructive = false,
  });

  final VoidCallback onPressed;
  final String label;
  final IconData? leadingIcon;
  final Widget? leading;
  final IconData? trailingIcon;
  // Reserve a fixed leading column so labels line up across items.
  final bool reserveLeadingSpace;
  final bool isDestructive;

  static const double defaultLeadingWidth = 16.0;

  bool get hasLeading => leading != null || leadingIcon != null;

  Widget? buildLeading(BuildContext context) {
    final widget = leading;
    if (widget != null) {
      return widget;
    }
    final icon = leadingIcon;
    if (icon != null) {
      return Icon(icon, size: defaultLeadingWidth);
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final leadingWidget = buildLeading(context);
    final colors = CustomColorScheme.of(context);
    final foregroundColor = isDestructive
        ? colors.function.danger
        : colors.text.primary;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: TextButton(
        onPressed: onPressed,
        style: TextButton.styleFrom(
          shape: const RoundedRectangleBorder(borderRadius: BorderRadius.zero),
          foregroundColor: foregroundColor,
          padding: const EdgeInsets.symmetric(vertical: Spacing.px4),
          alignment: Alignment.centerLeft,
          splashFactory: !Platform.isAndroid ? NoSplash.splashFactory : null,
          overlayColor: Colors.transparent,
        ),
        child: Row(
          mainAxisSize: MainAxisSize.max,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            if (reserveLeadingSpace) ...[
              SizedBox(width: defaultLeadingWidth, child: leadingWidget),
              const SizedBox(width: Spacing.px8),
            ] else if (leadingWidget != null) ...[
              leadingWidget,
              const SizedBox(width: Spacing.px8),
            ],
            Expanded(
              child: Text(
                label,
                style: TextStyle(fontSize: LabelFontSize.base.size),
                maxLines: 1,
                softWrap: false,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            if (trailingIcon != null) ...[
              const SizedBox(width: Spacing.px8),
              Icon(trailingIcon),
            ],
          ],
        ),
      ),
    );
  }

  ContextMenuItem copyWith({
    VoidCallback? onPressed,
    bool? reserveLeadingSpace,
    bool? isDestructive,
  }) {
    return ContextMenuItem(
      key: key,
      onPressed: onPressed ?? this.onPressed,
      label: label,
      leadingIcon: leadingIcon,
      leading: leading,
      trailingIcon: trailingIcon,
      reserveLeadingSpace: reserveLeadingSpace ?? this.reserveLeadingSpace,
      isDestructive: isDestructive ?? this.isDestructive,
    );
  }
}
