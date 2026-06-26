// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/modal/confirm_dialog.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'user_cubit.dart';

class RemoveUsernameDialog extends StatelessWidget {
  const RemoveUsernameDialog({super.key, required this.username});

  final UiUsername username;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ConfirmDialog(
      title: loc.removeUsernameDialog_title,
      message: loc.removeUsernameDialog_content,
      cancel: loc.removeUsernameDialog_cancel,
      confirm: loc.removeUsernameDialog_remove,
      destructive: true,
      onConfirm: () => context.read<UserCubit>().removeUsername(username),
    );
  }
}
