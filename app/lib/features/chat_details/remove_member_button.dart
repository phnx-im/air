// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class RemoveMemberButton extends StatelessWidget {
  const RemoveMemberButton({
    super.key,
    required this.chatId,
    required this.memberId,
    required this.displayName,
    this.enabled = true,
    this.compact = false,
    this.onRemoved,
  });

  final ChatId chatId;
  final UiUserId memberId;
  final String displayName;
  final bool enabled;
  final bool compact;
  final VoidCallback? onRemoved;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final button = Button(
      onPressed: () => _confirmRemoval(context),
      size: compact ? ButtonSize.small : ButtonSize.current,
      type: ButtonType.secondary,
      tone: ButtonTone.danger,
      state: enabled ? ButtonState.active : ButtonState.disabled,
      label: loc.removeUserButton_text,
    );

    // Compact shares its line with a member's name, so it has to hug its
    // label instead of taking the full width the row hands out.
    return compact ? IntrinsicWidth(child: button) : button;
  }

  Future<void> _confirmRemoval(BuildContext context) async {
    final loc = AppLocalizations.of(context);

    final confirmed = await showAdaptiveConfirm(
      context: context,
      title: loc.removeUserDialog_title,
      description: loc.removeUserDialog_content(displayName),
      primaryActionText: loc.removeUserDialog_removeUser,
      primaryTone: ButtonTone.danger,
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
