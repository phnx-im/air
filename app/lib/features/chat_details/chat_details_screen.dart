// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';

import 'package:air/features/developer/chat_debug_info_view.dart'
    show showChatDebugInfo;
import 'package:air/features/chat_details/contact_details_view.dart';
import 'package:air/features/chat_details/safety_code_screen.dart';
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
      UiChatType_TargetedMessageConnection(
        field0: final profile,
      ) => _ContactModal(profile: profile),
      UiChatType_Group() => const _GroupModal(),
      UiChatType_HandleConnection() ||
      UiChatType_PendingConnection() ||
      null => const _UnknownChatModal(),
    };
  }
}

/// The profile of the person on the other end of a one-to-one chat, and the
/// safety code that opens on top of it.
class _ContactModal extends StatelessWidget {
  const _ContactModal({required this.profile});

  final UiUserProfile profile;

  @override
  Widget build(BuildContext context) {
    final chat = context.select((ChatDetailsCubit cubit) => cubit.state.chat);
    if (chat == null) {
      return const SizedBox.shrink();
    }

    final safetyCodeOpen = context.select(
      (NavigationCubit cubit) => cubit.state.safetyCodeOpen,
    );
    final loc = AppLocalizations.of(context);

    return ModalScaffold(
      title: safetyCodeOpen
          ? loc.safetyCodeScreen_title
          : loc.contactDetailsScreen_title,
      onLeading: safetyCodeOpen ? () => _pop(context) : null,
      onTrailing: () => _pop(context),
      child: safetyCodeOpen
          ? SafetyCodeView(profile: profile)
          : ContactDetailsView(
              profile: profile,
              relationship: ContactRelationship(
                contactChatId: chat.id,
                isBlocked: chat.status == const UiChatStatus.blocked(),
              ),
              onNameLongPress: () => showChatDebugInfo(context, chat),
            ),
    );
  }
}

class _GroupModal extends StatelessWidget {
  const _GroupModal();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ModalScaffold(
      title: loc.groupDetails_title,
      onTrailing: () => _pop(context),
      child: const GroupDetailsScreen(),
    );
  }
}

/// Fallback for a chat we have no details surface for, which is a connection
/// that has not been accepted yet.
class _UnknownChatModal extends StatelessWidget {
  const _UnknownChatModal();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ModalScaffold(
      title: loc.contactDetailsScreen_title,
      onTrailing: () => _pop(context),
      child: Center(child: Text(loc.chatDetailsScreen_unknownChat)),
    );
  }
}

/// Closes the topmost modal. The navigation state owns the stack, so the
/// header's actions go through it rather than through the navigator.
void _pop(BuildContext context) => context.read<NavigationCubit>().pop();
