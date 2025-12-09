// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
import 'package:air/ui/typography/font_size.dart';
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
    final loc = AppLocalizations.of(context);

    final colors = CustomColorScheme.of(context);

    final isDesktop = ResponsiveScreen.isDesktop(context);

    return OutlinedButton(
      onPressed: () => _confirmRemoval(context),
      style: ButtonStyle(
        padding: WidgetStatePropertyAll(
          compact
              ? const EdgeInsets.symmetric(
                  horizontal: Spacings.s,
                  vertical: Spacings.xxxs,
                )
              : null,
        ),
        shape: compact
            ? WidgetStatePropertyAll(
                RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
              )
            : null,
        minimumSize: compact
            ? null
            : WidgetStatePropertyAll(
                Size(isDesktop ? 320 : double.infinity, 0),
              ),
        visualDensity: compact ? VisualDensity.compact : null,
        backgroundColor: WidgetStatePropertyAll(
          compact ? colors.backgroundBase.secondary : colors.function.danger,
        ),
        overlayColor: WidgetStatePropertyAll(
          compact ? colors.backgroundBase.secondary : colors.function.danger,
        ),
      ),
      child: Text(
        loc.removeUserButton_text,
        style: TextStyle(
          fontSize: compact
              ? LabelFontSize.small1.size
              : LabelFontSize.base.size,
          color: compact ? colors.text.primary : colors.function.white,
        ),
      ),
    );
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
