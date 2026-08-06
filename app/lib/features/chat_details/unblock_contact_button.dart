// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class UnblockContactButton extends StatelessWidget {
  const UnblockContactButton({
    required this.userId,
    required this.displayName,
    super.key,
  });

  final UiUserId userId;
  final String displayName;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    // Constructive, so it stays neutral where its counterpart reads as danger.
    return Button(
      onPressed: () => _unblock(context),
      size: ButtonSize.current,
      type: ButtonType.secondary,
      label: loc.unblockContactButton_text,
    );
  }

  void _unblock(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final loc = AppLocalizations.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ConfirmDialog(
        title: loc.unblockContactDialog_title(displayName),
        message: loc.unblockContactDialog_content(displayName),
        cancel: loc.unblockContactDialog_cancel,
        confirm: loc.unblockContactDialog_unblock,
      ),
    );
    if (confirmed ?? false) {
      userCubit.unblockContact(userId);
    }
  }
}
