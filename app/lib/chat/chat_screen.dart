// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/message_list.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/app_bar_back_button.dart';
import 'package:air/widgets/avatar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'chat_details_cubit.dart';
import 'delete_contact_button.dart';
import 'report_spam_button.dart';
import 'unblock_contact_button.dart';

class ChatScreen extends StatelessWidget {
  const ChatScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final chatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );

    if (chatId == null) {
      return const _EmptyChatPane();
    }

    return MultiBlocProvider(
      providers: [
        BlocProvider(
          // rebuilds the cubit when a different chat is selected
          key: ValueKey("message-list-cubit-$chatId"),
          create: (context) => ChatDetailsCubit(
            userCubit: context.read<UserCubit>(),
            userSettingsCubit: context.read<UserSettingsCubit>(),
            chatId: chatId,
            chatsRepository: context.read<ChatsRepository>(),
            attachmentsRepository: context.read<AttachmentsRepository>(),
          ),
        ),
        BlocProvider(
          // rebuilds the cubit when a different chat is selected
          key: ValueKey("message-list-cubit-$chatId"),
          create: (context) => MessageListCubit(
            userCubit: context.read<UserCubit>(),
            chatId: chatId,
          ),
        ),
      ],
      child: const ChatScreenView(),
    );
  }
}

class _EmptyChatPane extends StatelessWidget {
  const _EmptyChatPane();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return Center(
      child: Text(
        style: Theme.of(context).textTheme.bodyLarge?.copyWith(
          color: CustomColorScheme.of(context).text.tertiary,
        ),
        loc.chatScreen_emptyChat,
      ),
    );
  }
}

class ChatScreenView extends StatelessWidget {
  const ChatScreenView({super.key, this.createMessageCubit = MessageCubit.new});

  final MessageCubitCreate createMessageCubit;

  @override
  Widget build(BuildContext context) {
    final chatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );
    final ownUserId = context.select((UserCubit cubit) => cubit.state.userId);
    if (chatId == null) {
      return const _EmptyChatPane();
    }

    final (
      :status,
      :blockedUserId,
      :blockedUserDisplayName,
      :members,
      :isGroupChat,
    ) = context.select((ChatDetailsCubit cubit) {
      final chat = cubit.state.chat;
      final status = chat?.status;
      return (
        status: status,
        blockedUserId: switch (status) {
          UiChatStatus_Blocked() => chat?.userId,
          _ => null,
        },
        blockedUserDisplayName: switch (status) {
          UiChatStatus_Blocked() => chat?.displayName,
          _ => null,
        },
        members: cubit.state.members,
        isGroupChat: switch (chat?.chatType) {
          UiChatType_Group() => true,
          _ => false,
        },
      );
    });

    final bool isCurrentUserMember =
        members.isEmpty || members.contains(ownUserId);

    final bool showInactiveFooter =
        switch (status) {
          UiChatStatus_Inactive() => true,
          _ => false,
        } ||
        (isGroupChat && !isCurrentUserMember);

    final bool showBlockedFooter =
        blockedUserId != null && blockedUserDisplayName != null;

    Widget footer = const MessageComposer();
    if (showInactiveFooter) {
      footer = const _InactiveChatFooter();
    } else if (showBlockedFooter) {
      footer = _BlockedChatFooter(
        chatId: chatId,
        userId: blockedUserId,
        displayName: blockedUserDisplayName,
      );
    }

    return Scaffold(
      appBar: _ChatHeader(),
      body: SafeArea(
        minimum: const EdgeInsets.only(bottom: Spacings.xs),
        child: Column(
          children: [
            Expanded(
              child: MessageListView(createMessageCubit: createMessageCubit),
            ),
            footer,
          ],
        ),
      ),
    );
  }
}

class _ChatHeader extends StatelessWidget implements PreferredSizeWidget {
  _ChatHeader();

  final GlobalKey _key = GlobalKey();

  @override
  Widget build(BuildContext context) {
    final (chatId, title, image) = context.select(
      (ChatDetailsCubit cubit) => (
        cubit.state.chat?.id,
        cubit.state.chat?.title,
        cubit.state.chat?.picture,
      ),
    );

    return AppBar(
      key: _key,
      automaticallyImplyLeading: false,
      backgroundColor: CustomColorScheme.of(context).backgroundBase.primary,
      surfaceTintColor: Colors.transparent,
      scrolledUnderElevation: 0,
      elevation: 0,
      leading: context.responsiveScreenType == ResponsiveScreenType.mobile
          ? const AppBarBackButton()
          : const SizedBox.shrink(),
      centerTitle: true,
      title: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () {
            context.read<NavigationCubit>().openChatDetails();
          },
          child: Row(
            mainAxisSize: MainAxisSize.min,
            spacing: Spacings.xs,
            children: [
              GroupAvatar(chatId: chatId, size: Spacings.l),
              Flexible(
                child: Text(
                  title ?? "",
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextTheme.of(context).labelMedium!.copyWith(
                    color: CustomColorScheme.of(context).text.tertiary,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  @override
  Size get preferredSize => const Size.fromHeight(kToolbarHeight);
}

class _BlockedChatFooter extends StatelessWidget {
  const _BlockedChatFooter({
    required this.chatId,
    required this.userId,
    required this.displayName,
  });

  final ChatId chatId;
  final UiUserId userId;
  final String displayName;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final buttonWidth = isSmallScreen(context) ? double.infinity : null;
    return Container(
      padding: const EdgeInsets.all(Spacings.s),
      child: Column(
        children: [
          Text(loc.blockedChatFooter_message(displayName)),
          const SizedBox(height: Spacings.s),
          Wrap(
            runSpacing: Spacings.xxs,
            alignment: WrapAlignment.center,
            children: [
              SizedBox(
                width: buttonWidth,
                child: DeleteContactButton(
                  chatId: chatId,
                  displayName: displayName,
                ),
              ),
              const SizedBox(width: Spacings.s),
              SizedBox(
                width: buttonWidth,
                child: ReportSpamButton(userId: userId),
              ),
              const SizedBox(width: Spacings.s),
              SizedBox(
                width: buttonWidth,
                child: UnblockContactButton(
                  userId: userId,
                  displayName: displayName,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _InactiveChatFooter extends StatelessWidget {
  const _InactiveChatFooter();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(Spacings.s),
      child: Text(
        loc.inactiveChatFooter_message,
        textAlign: TextAlign.center,
        style: Theme.of(context).textTheme.bodyMedium?.copyWith(
          color: CustomColorScheme.of(context).text.tertiary,
        ),
      ),
    );
  }
}
