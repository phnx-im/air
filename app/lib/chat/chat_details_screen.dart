// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/navigation/navigation.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';

import 'contact_details.dart';
import 'chat_details_cubit.dart';
import 'group_details.dart';

/// Container for [ChatDetailsScreenView]
///
/// Wraps the screen with required providers.
class ChatDetailsScreen extends StatelessWidget {
  const ChatDetailsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final chatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );

    if (chatId == null) {
      return const SizedBox.shrink();
    }

    return BlocProvider(
      create: (context) {
        return ChatDetailsCubit(
          userCubit: context.read<UserCubit>(),
          userSettingsCubit: context.read<UserSettingsCubit>(),
          chatId: chatId,
          chatsRepository: context.read<ChatsRepository>(),
          attachmentsRepository: context.read<AttachmentsRepository>(),
        );
      },
      child: const ChatDetailsScreenView(),
    );
  }
}

/// Screen that shows details of a chat
class ChatDetailsScreenView extends StatelessWidget {
  const ChatDetailsScreenView({super.key});

  @override
  Widget build(BuildContext context) {
    final chatType = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.chatType,
    );

    return AppScaffold(
      child: switch (chatType) {
        UiChatType_Connection(field0: final profile) ||
        UiChatType_TargetedMessageConnection(field0: final profile) => Builder(
          builder: (context) {
            final chat = context.select(
              (ChatDetailsCubit cubit) => cubit.state.chat,
            );
            if (chat == null) {
              return const SizedBox.shrink();
            }
            return ContactDetailsView(
              profile: profile,
              relationship: ContactRelationship(
                contactChatId: chat.id,
                isBlocked: chat.status == const UiChatStatus.blocked(),
              ),
            );
          },
        ),
        UiChatType_Group() => const GroupDetails(),
        UiChatType_HandleConnection() || null => Builder(
          builder: (context) {
            final loc = AppLocalizations.of(context);
            return Center(child: Text(loc.chatDetailsScreen_unknownChat));
          },
        ),
      },
    );
  }
}
