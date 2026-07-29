// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';

import 'package:air/features/developer/chat_debug_info_view.dart';
import 'package:air/features/chat_details/contact_details_view.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/group_details_screen.dart';

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

    return switch (chatType) {
      UiChatType_Connection(field0: final profile) ||
      UiChatType_TargetedMessageConnection(field0: final profile) => Builder(
        builder: (context) {
          final chat = context.select(
            (ChatDetailsCubit cubit) => cubit.state.chat,
          );
          if (chat == null) {
            return const SizedBox.shrink();
          }
          return AppScaffold(
            title: chat.title,
            onTitleLongPress: () {
              final chatDetailsCubit = context.read<ChatDetailsCubit>();
              final userCubit = context.read<UserCubit>();
              Navigator.of(context).push(
                MaterialPageRoute(
                  builder: (context) => ChatDebugInfoView(
                    title: chat.title,
                    loadDebugInfo: () => chatDetailsCubit.chatDebugInfo(),
                    onUpdateGroup: () => chatDetailsCubit.updateKey(),
                    onUpdateApqGroup: () => chatDetailsCubit.updateApqKey(),
                    onRequestResync: () => chatDetailsCubit.requestResync(),
                    onEraseLocalChat: () => userCubit.devEraseChat(chat.id),
                  ),
                ),
              );
            },
            child: ContactDetailsView(
              profile: profile,
              relationship: ContactRelationship(
                contactChatId: chat.id,
                isBlocked: chat.status == const UiChatStatus.blocked(),
              ),
            ),
          );
        },
      ),
      UiChatType_Group() => const GroupDetailsScreen(),
      UiChatType_HandleConnection() ||
      UiChatType_PendingConnection() ||
      null => Builder(
        builder: (context) {
          final loc = AppLocalizations.of(context);
          return AppScaffold(
            child: Center(child: Text(loc.chatDetailsScreen_unknownChat)),
          );
        },
      ),
    };
  }
}
