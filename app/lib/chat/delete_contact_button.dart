// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/util/dialog.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class DeleteContactButton extends StatelessWidget {
  const DeleteContactButton({
    required this.chatId,
    required this.displayName,

    super.key,
  });

  final ChatId chatId;
  final String displayName;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return OutlinedButton(
      onPressed: () => _delete(context),
      child: Text(
        loc.deleteContactButton_text,
        style: TextStyle(color: CustomColorScheme.of(context).function.danger),
      ),
    );
  }

  void _delete(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    final loc = AppLocalizations.of(context);
    final confirmed = await showConfirmationDialog(
      context,
      title: loc.deleteContactDialog_title,
      message: loc.deleteContactDialog_content(displayName),
      positiveButtonText: loc.deleteContactDialog_delete,
      negativeButtonText: loc.deleteContactDialog_cancel,
    );
    if (confirmed) {
      userCubit.deleteChat(chatId);
      navigationCubit.closeChat();
    }
  }
}
