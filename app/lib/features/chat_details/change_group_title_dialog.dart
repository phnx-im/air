// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/ds/patterns/edit_dialog/edit_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

import 'package:air/features/chat/chat_details_cubit.dart';

final _log = Logger('ChangeGroupTitleDialog');

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
    final navigator = Navigator.of(context);
    // The dialog is closed by the time an error comes back, so the error
    // dialog is shown from its navigator.
    final navigatorContext = navigator.context;
    navigator.pop();
    try {
      await chatDetailsCubit.setChatTitle(title: text.trim());
    } catch (e, stackTrace) {
      _log.severe("Failed to change the group title: $e", e, stackTrace);
      if (!navigatorContext.mounted) return;
      showErrorDialog(
        navigatorContext,
        message: (loc) => loc.errorDialog_tryAgainLater,
      );
    }
  }
}
