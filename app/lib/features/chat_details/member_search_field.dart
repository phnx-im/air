// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/components/searchfield/searchfield.dart';
import 'package:air/ds/components/searchfield/searchfield_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';

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
    return Padding(
      padding: const EdgeInsets.only(
        left: S.s16,
        right: S.s16,
        top: S.s24,
        bottom: S.s8,
      ),
      child: SearchField(
        tokens: SearchFieldTokens.of(context),
        controller: controller,
        hintText: hintText,
        onChanged: onChanged,
      ),
    );
  }
}
