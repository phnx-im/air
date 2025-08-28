// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/components/context_menu/context_menu_ui.dart';

class ContextMenuShowcase extends StatelessWidget {
  const ContextMenuShowcase({super.key});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 200,
      child: ContextMenuUi(
        onHide: () {},
        menuItems: [
          ContextMenuItem(
            onPressed: () {},
            leadingIcon: Icons.person_outline_rounded,
            label: 'Action 1',
          ),
          ContextMenuItem(
            onPressed: () {},
            leadingIcon: Icons.settings_outlined,
            label: 'Action 2',
          ),
        ],
      ),
    );
  }
}
