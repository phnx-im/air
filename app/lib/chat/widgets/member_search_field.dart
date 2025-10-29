// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;

class MemberSearchField extends StatelessWidget {
  const MemberSearchField({
    super.key,
    required this.controller,
    required this.hintText,
    required this.onChanged,
  });

  final TextEditingController controller;
  final String hintText;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final customColorScheme = CustomColorScheme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Spacings.m,
        Spacings.m,
        Spacings.m,
        Spacings.xxs,
      ),
      child: TextField(
        controller: controller,
        onChanged: onChanged,
        decoration: InputDecoration(
          isDense: true,
          visualDensity: VisualDensity.compact,
          prefixIcon: Padding(
            padding: const EdgeInsets.all(8.0),
            child: iconoir.Search(
              width: 16,
              height: 16,
              color: customColorScheme.text.primary,
            ),
          ),
          prefixIconConstraints: const BoxConstraints(
            minWidth: 28,
            minHeight: 28,
          ),
          hintText: hintText,
          hintStyle: Theme.of(context).textTheme.bodyMedium?.copyWith(
            color: customColorScheme.text.quaternary,
          ),
          border: OutlineInputBorder(borderRadius: BorderRadius.circular(12)),
        ),
      ),
    );
  }
}
