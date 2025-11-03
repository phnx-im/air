// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
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
    if (!enabled) return const SizedBox.shrink();

    final button = OutlinedButton(
      style: OutlinedButton.styleFrom(
        padding: EdgeInsets.symmetric(
          horizontal: compact ? Spacings.s : Spacings.m,
          vertical: compact ? Spacings.xxxs : Spacings.s,
        ),
        visualDensity: compact ? VisualDensity.compact : VisualDensity.standard,
        textStyle: compact ? Theme.of(context).textTheme.labelSmall! : null,
        shape:
            compact
                ? RoundedRectangleBorder(borderRadius: BorderRadius.circular(8))
                : null,
      ),
      onPressed: () => _confirmRemoval(context),
      child: Text(AppLocalizations.of(context).removeUserButton_text),
    );

    return button;
  }

  Future<void> _confirmRemoval(BuildContext context) async {
    final loc = AppLocalizations.of(context);

    final confirmed = await showBottomSheetDialog(
      context: context,
      title: loc.removeUserDialog_title,
      description: loc.removeUserDialog_content(displayName),
      primaryActionText: loc.removeUserDialog_removeUser,
      isPrimaryDanger: true,
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
