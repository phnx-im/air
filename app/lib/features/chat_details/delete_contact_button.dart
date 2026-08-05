// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
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

    return Button(
      onPressed: () => _delete(context),
      size: ButtonSize.current,
      type: ButtonType.secondary,
      tone: ButtonTone.danger,
      label: loc.deleteContactButton_text,
    );
  }

  void _delete(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    final loc = AppLocalizations.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ConfirmDialog(
        title: loc.deleteContactDialog_title,
        message: loc.deleteContactDialog_content(displayName),
        cancel: loc.deleteContactDialog_cancel,
        confirm: loc.deleteContactDialog_delete,
        destructive: true,
      ),
    );
    if (confirmed ?? false) {
      AppHaptics.destructive();
      userCubit.deleteChat(chatId);
      navigationCubit.closeChat();
    }
  }
}
