// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'user_cubit.dart';

class RemoveUsernameDialog extends StatelessWidget {
  const RemoveUsernameDialog({super.key, required this.username});

  final UiUserHandle username;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              loc.removeUsernameDialog_title,
              style: TextStyle(
                fontSize: HeaderFontSize.h4.size,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),

          const SizedBox(height: Spacings.xxs),

          Text(
            loc.removeUsernameDialog_content,
            style: TextStyle(
              color: colors.text.secondary,
              fontSize: BodyFontSize.base.size,
            ),
          ),

          const SizedBox(height: Spacings.m),

          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  child: Text(loc.removeUsernameDialog_cancel),
                ),
              ),

              const SizedBox(width: Spacings.xs),

              Expanded(
                child: OutlinedButton(
                  onPressed: () {
                    context.read<UserCubit>().removeUserHandle(username);
                    Navigator.of(context).pop(true);
                  },
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.function.danger,
                    ),
                    foregroundColor: WidgetStatePropertyAll(
                      colors.function.white,
                    ),
                  ),
                  child: Text(loc.removeUsernameDialog_remove),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
