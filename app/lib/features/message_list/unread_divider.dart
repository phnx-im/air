// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';

class UnreadDivider extends StatelessWidget {
  const UnreadDivider({super.key, required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    final label = AppLocalizations.of(context).messageList_newMessages(count);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s24, vertical: S.s32),
      child: Row(
        children: [
          Expanded(
            child: Divider(color: SemanticColors.of(context).separator.primary),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: S.s16),
            child: DecoratedBox(
              decoration: ShapeDecoration(
                color: SemanticColors.of(context).function.neutral.toggleBlack,
                shape: const StadiumBorder(),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: S.s16,
                  vertical: S.s4,
                ),
                child: Text(
                  label,
                  style: TextTheme.of(context).bodySmall?.copyWith(
                    color: SemanticColors.of(
                      context,
                    ).function.neutral.toggleWhite,
                  ),
                ),
              ),
            ),
          ),
          Expanded(
            child: Divider(color: SemanticColors.of(context).separator.primary),
          ),
        ],
      ),
    );
  }
}
