// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';

class UnreadDivider extends StatelessWidget {
  const UnreadDivider({super.key, required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    final label = AppLocalizations.of(
      context,
    ).messageList_unreadMessages(count);
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.m,
        vertical: Spacings.l,
      ),
      child: Row(
        children: [
          Expanded(
            child: Divider(
              color: CustomColorScheme.of(context).separator.primary,
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
            child: DecoratedBox(
              decoration: ShapeDecoration(
                color: CustomColorScheme.of(context).function.toggleBlack,
                shape: const StadiumBorder(),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: Spacings.s,
                  vertical: Spacings.xxxs,
                ),
                child: Text(
                  label,
                  style: TextTheme.of(context).bodySmall?.copyWith(
                    color: CustomColorScheme.of(context).text.primary,
                  ),
                ),
              ),
            ),
          ),
          Expanded(
            child: Divider(
              color: CustomColorScheme.of(context).separator.primary,
            ),
          ),
        ],
      ),
    );
  }
}
