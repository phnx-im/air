// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

import 'block_contact_button.dart';
import 'chat_details_cubit.dart';
import 'delete_contact_button.dart';
import 'report_spam_button.dart';
import 'unblock_contact_button.dart';

final _log = Logger('ConnectionDetails');

/// Details of a 1:1 connection
class ConnectionDetails extends StatelessWidget {
  const ConnectionDetails({super.key});

  @override
  Widget build(BuildContext context) {
    final chat = context.select((ChatDetailsCubit cubit) => cubit.state.chat);

    if (chat == null) {
      return const SizedBox.shrink();
    }

    final profile = switch (chat.chatType) {
      UiChatType_Connection(field0: final profile) => profile,
      _ => null,
    };
    if (profile == null) {
      _log.warning("profile is null in 1:1 connection details");
      return const SizedBox.shrink();
    }

    final isBlocked = chat.status == const UiChatStatus.blocked();

    return Center(
      child: Column(
        children: [
          const SizedBox(height: Spacings.l),
          UserAvatar(
            size: 128,
            displayName: profile.displayName,
            image: profile.profilePicture,
          ),
          const SizedBox(height: Spacings.l),
          Text(
            style: Theme.of(context).textTheme.bodyLarge,
            profile.displayName,
          ),
          const SizedBox(height: Spacings.l),
          Text(
            chat.chatType.description,
            style: Theme.of(context).textTheme.bodyMedium,
          ),

          const Spacer(),

          isBlocked
              ? UnblockContactButton(
                userId: profile.userId,
                displayName: profile.displayName,
              )
              : BlockContactButton(
                userId: profile.userId,
                displayName: profile.displayName,
              ),
          const SizedBox(height: Spacings.s),

          DeleteContactButton(
            chatId: chat.id,
            displayName: profile.displayName,
          ),
          const SizedBox(height: Spacings.s),

          ReportSpamButton(userId: profile.userId),
          const SizedBox(height: Spacings.s),
        ],
      ),
    );
  }
}
