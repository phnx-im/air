// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:flutter/material.dart';

class UnreadDivider extends StatelessWidget {
  const UnreadDivider({super.key, required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    final label = AppLocalizations.of(context).messageList_newMessages(count);
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.px24,
        vertical: Spacing.px32,
      ),
      child: Row(
        children: [
          Expanded(
            child: Divider(
              color: CustomColorScheme.of(context).separator.primary,
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacing.px16),
            child: DecoratedBox(
              decoration: ShapeDecoration(
                color: CustomColorScheme.of(context).function.toggleBlack,
                shape: const StadiumBorder(),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: Spacing.px16,
                  vertical: Spacing.px4,
                ),
                child: Text(
                  label,
                  style: TextTheme.of(context).bodySmall?.copyWith(
                    color: CustomColorScheme.of(context).function.toggleWhite,
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
