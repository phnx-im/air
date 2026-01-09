// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/theme/spacings.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/effects/elevation.dart';

class ContextMenuUi extends StatelessWidget {
  const ContextMenuUi({
    super.key,
    required this.menuItems,
    required this.onHide,
    this.maxHeight,
  });

  final List<ContextMenuEntry> menuItems;
  final VoidCallback onHide;
  final double? maxHeight;

  @override
  Widget build(BuildContext context) {
    // Only reserve a leading column when any item uses it so labels align.
    final hasAnyLeading = menuItems.whereType<ContextMenuItem>().any(
      (item) => item.hasLeading || item.reserveLeadingSpace,
    );
    // Render entries as provided so callers can interleave separators with items.
    final items = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final entry in menuItems)
          if (entry is ContextMenuItem)
            hasAnyLeading ? entry.copyWith(reserveLeadingSpace: true) : entry
          else
            entry,
      ],
    );

    // Constrain and scroll when the menu would exceed available viewport height.
    final body = maxHeight == null
        ? items
        : ConstrainedBox(
            constraints: BoxConstraints(maxHeight: maxHeight!),
            child: SingleChildScrollView(child: items),
          );

    return Container(
      clipBehavior: Clip.hardEdge,
      decoration: BoxDecoration(
        color: CustomColorScheme.of(context).backgroundElevated.primary,
        boxShadow: elevationBoxShadows(context),
        borderRadius: BorderRadius.circular(16),
      ),
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.s,
        vertical: Spacings.xxs,
      ),
      child: body,
    );
  }
}
