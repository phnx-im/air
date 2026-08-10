// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_stack.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/add_members_pane.dart';
import 'package:air/features/chat_details/chat_details_pane.dart';
import 'package:air/features/chat_details/group_members_pane.dart';
import 'package:air/features/chat_details/member_details_cubit.dart';
import 'package:air/features/chat_details/member_details_pane.dart';
import 'package:air/features/chat_details/safety_code_pane.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// The chat details drill-down as one modal that pages, for the card
/// presentation.
///
/// Every level shares the one route, so the cubits are built once and the card
/// animates between levels instead of being replaced.
class ChatDetailsModal extends StatelessWidget {
  const ChatDetailsModal({
    super.key,
    required this.chatId,
    required this.pages,
  });

  final ChatId chatId;

  /// The drill-down, bottom level first.
  final List<ChatDetailsPage> pages;

  @override
  Widget build(BuildContext context) {
    final navigationCubit = context.read<NavigationCubit>();

    return ChatDetailsScope(
      chatId: chatId,
      child: ModalPageStack(
        onBack: navigationCubit.pop,
        onDismiss: navigationCubit.closeChatDetails,
        pages: [
          for (final page in pages)
            ModalStackEntry(key: page.paneKey, child: page.pane(chatId)),
        ],
      ),
    );
  }
}

/// One level of the drill-down as a route of its own, for the full-screen
/// presentation: each level is a push like any other.
class ChatDetailsModalScreen extends StatelessWidget {
  const ChatDetailsModalScreen({
    super.key,
    required this.chatId,
    required this.page,
  });

  final ChatId chatId;
  final ChatDetailsPage page;

  @override
  Widget build(BuildContext context) {
    return ChatDetailsScope(
      chatId: chatId,
      child: ModalSurface(
        // The route below is the previous level, so closing and going back
        // are the same move.
        child: ModalPageActions(
          onDismiss: context.read<NavigationCubit>().pop,
          child: page.pane(chatId),
        ),
      ),
    );
  }
}

/// The cubits the levels of the drill-down read, built over all of them.
class ChatDetailsScope extends StatelessWidget {
  const ChatDetailsScope({
    super.key,
    required this.chatId,
    required this.child,
  });

  final ChatId chatId;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return MultiBlocProvider(
      // Keyed on the chat, so switching chats rebuilds the cubits.
      key: ValueKey(chatId),
      providers: [
        BlocProvider(
          create: (context) => ChatDetailsCubit(
            userCubit: context.read<UserCubit>(),
            userSettingsCubit: context.read<UserSettingsCubit>(),
            chatsRepository: context.read<ChatsRepository>(),
            attachmentsRepository: context.read<AttachmentsRepository>(),
            chatId: chatId,
          ),
        ),
        BlocProvider(
          create: (context) => MemberDetailsCubit(
            userCubit: context.read<UserCubit>(),
            chatId: chatId,
          ),
        ),
      ],
      child: child,
    );
  }
}

/// How a level of the drill-down is presented.
extension ChatDetailsPagePresentation on ChatDetailsPage {
  /// The page's content, on whichever surface holds it.
  Widget pane(ChatId chatId) => switch (this) {
    DetailsPage() => const ChatDetailsPane(),
    GroupMembersPage() => GroupMembersPane(chatId: chatId),
    AddMembersPage() => AddMembersPane(chatId: chatId),
    MemberDetailsPage(:final member) => MemberDetailsPane(
      chatId: chatId,
      memberId: member,
    ),
    SafetyCodePage(:final user) => SafetyCodePane(user: user),
  };

  /// Identifies the level, so what it holds survives being covered.
  ValueKey<String> get paneKey => switch (this) {
    DetailsPage() => const ValueKey("chat-details"),
    GroupMembersPage() => const ValueKey("chat-group-members"),
    AddMembersPage() => const ValueKey("chat-add-members"),
    MemberDetailsPage(:final member) => ValueKey(
      "chat-member-details-${member.uuid}",
    ),
    SafetyCodePage(:final user) => ValueKey("chat-safety-code-${user.uuid}"),
  };
}
