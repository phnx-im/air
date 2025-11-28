// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/ui/components/modal/edit_dialog.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

class ChangeDisplayNameDialog extends HookWidget {
  const ChangeDisplayNameDialog({super.key, required this.displayName});

  final String displayName;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return EditDialog(
      title: loc.editDisplayNameScreen_title,
      description: loc.editDisplayNameScreen_description,
      cancel: loc.editDisplayNameScreen_cancel,
      confirm: loc.editDisplayNameScreen_save,
      initialValue: displayName,
      validator: (value) => value.trim().isNotEmpty,
      onSubmit: (value) => _submit(context, value.trim()),
    );
  }

  void _submit(BuildContext context, String text) {
    if (text.trim().isEmpty) return;
    final userCubit = context.read<UserCubit>();
    userCubit.setProfile(displayName: text.trim());
    Navigator.of(context).pop();
  }
}
