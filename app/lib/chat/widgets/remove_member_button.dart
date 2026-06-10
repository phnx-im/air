// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart'
    show AppButtonTone, AppButton, AppButtonSize;
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class RemoveMemberButton extends StatelessWidget {
  const RemoveMemberButton({
    super.key,
    required this.chatId,
    required this.memberId,
    required this.displayName,
    this.enabled = true,
    this.size = .large,
    this.onRemoved,
  });

  final ChatId chatId;
  final UiUserId memberId;
  final String displayName;
  final bool enabled;
  final AppButtonSize size;
  final VoidCallback? onRemoved;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return AppButton(
      onPressed: () => _confirmRemoval(context),
      label: loc.removeUserButton_text,
      type: .secondary,
      tone: .danger,
      size: size,
    );
  }

  Future<void> _confirmRemoval(BuildContext context) async {
    final loc = AppLocalizations.of(context);

    final confirmed = await showBottomSheetDialog(
      context: context,
      title: loc.removeUserDialog_title,
      description: loc.removeUserDialog_content(displayName),
      primaryActionText: loc.removeUserDialog_removeUser,
      primaryTone: AppButtonTone.danger,
      onPrimaryAction: (actionContext) async {
        await actionContext.read<UserCubit>().removeUserFromChat(
          chatId,
          memberId,
        );
      },
    );

    if (confirmed && onRemoved != null) {
      onRemoved!();
    }
  }
}
