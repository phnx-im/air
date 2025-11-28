// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/ui/components/modal/edit_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

import 'chat_details_cubit.dart';

class ChangeGroupTitleDialog extends HookWidget {
  const ChangeGroupTitleDialog({super.key, required this.groupTitle});

  final String groupTitle;

  @override
  Widget build(BuildContext context) {
    return EditDialog(
      title: AppLocalizations.of(context).changeGroupTitleDialog_title,
      description: AppLocalizations.of(context).changeGroupTitleDialog_content,
      cancel: AppLocalizations.of(context).changeGroupTitleDialog_cancel,
      confirm: AppLocalizations.of(context).changeGroupTitleDialog_confirm,
      initialValue: groupTitle,
      validator: (value) => value.trim().isNotEmpty,
      onSubmit: (value) => _submit(context, value),
    );
  }

  void _submit(BuildContext context, String text) async {
    if (text.trim().isEmpty) return;
    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    chatDetailsCubit.setChatTitle(title: text.trim());
    Navigator.of(context).pop();
  }
}
