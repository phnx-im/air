// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/chat_list/chat_list_item_tokens.dart';
import 'package:air/ds/patterns/chat_list/chat_list_tokens.dart';
import 'package:air/ds/patterns/reminder_banner/reminder_banner.dart';
import 'package:air/ds/patterns/reminder_banner/reminder_banner_tokens.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/app_store.dart';
import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';

class UpdateReminder extends StatelessWidget {
  const UpdateReminder({super.key, required this.expiresAt});

  final DateTime expiresAt;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final base = ReminderBannerTokens.defaults(SemanticPalette.of(context));
    final row = ChatListItemTokens.current;

    return ReminderBanner(
      // Line up the icon with the avatar below and the text with the chat titles.
      tokens: base.copyWith(
        padding: base.padding.copyWith(
          left:
              row.containerPadding.left - ChatListTokens.topBannerPadding.left,
        ),
        leadingColumnWidth: row.avatarSize,
        gap: ChatListItemTokens.avatarPadding.right,
      ),
      type: .refreshCw,
      title: loc.updateReminder_title,
      body: loc.updateReminder_body,
      onTap: DeviceType.isPhone ? openAppStore : null,
      onDismiss: () => context
          .read<UserSettingsCubit>()
          .setDismissedVersionExpiry(value: expiresAt),
    );
  }
}
