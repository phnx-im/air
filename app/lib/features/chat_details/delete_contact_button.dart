// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/ds/patterns/dialog/show_confirmation_dialog.dart';
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

    final colors = SemanticColors.of(context);

    final isDesktop = DeviceType.isDesktop;

    return OutlinedButton(
      onPressed: () => _delete(context),
      style: ButtonStyle(
        minimumSize: WidgetStatePropertyAll(
          Size(isDesktop ? 320 : double.infinity, 0),
        ),
        backgroundColor: WidgetStatePropertyAll(colors.function.danger),
        overlayColor: WidgetStatePropertyAll(colors.function.danger),
      ),
      child: Text(
        loc.deleteContactButton_text,
        style: typeScale.body.regular.style(
          color: colors.function.neutral.white,
        ),
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
      AppHaptics.destructive();
      userCubit.deleteChat(chatId);
      navigationCubit.closeChat();
    }
  }
}
