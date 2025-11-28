// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'package:flutter/material.dart';
import 'package:air/theme/spacings.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';

class ContextMenuItem extends StatelessWidget {
  const ContextMenuItem({
    super.key,
    required this.onPressed,
    required this.label,
    this.leadingIcon,
    this.leading,
    this.trailingIcon,
  });

  final VoidCallback onPressed;
  final String label;
  final IconData? leadingIcon;
  final Widget? leading;
  final IconData? trailingIcon;

  @override
  Widget build(BuildContext context) {
    return TextButton(
      onPressed: onPressed,
      style: TextButton.styleFrom(
        shape: const RoundedRectangleBorder(borderRadius: BorderRadius.zero),
        foregroundColor: CustomColorScheme.of(context).text.primary,
        padding: const EdgeInsets.symmetric(vertical: Spacings.s),
        alignment: Alignment.centerLeft,
        splashFactory: !Platform.isAndroid ? NoSplash.splashFactory : null,
        overlayColor: Colors.transparent,
      ),
      child: Row(
        mainAxisSize: MainAxisSize.max,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          if (leading != null) ...[
            leading!,
            const SizedBox(width: Spacings.xxs),
          ] else if (leadingIcon != null) ...[
            Icon(leadingIcon, size: 24),
            const SizedBox(width: Spacings.xxs),
          ],
          Expanded(
            child: Text(
              label,
              style: TextStyle(fontSize: LabelFontSize.base.size),
            ),
          ),
          if (trailingIcon != null) ...[
            const SizedBox(width: Spacings.xxs),
            Icon(trailingIcon),
          ],
        ],
      ),
    );
  }

  ContextMenuItem copyWith({required Null Function() onPressed}) {
    return ContextMenuItem(
      onPressed: onPressed,
      label: label,
      leadingIcon: leadingIcon,
      leading: leading,
      trailingIcon: trailingIcon,
    );
  }
}
