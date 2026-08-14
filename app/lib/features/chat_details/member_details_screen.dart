// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/chat_details/contact_details_view.dart';
import 'package:air/features/chat_details/safety_code_screen.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/member_details_cubit.dart';

class MemberDetailsScreen extends StatelessWidget {
  const MemberDetailsScreen({
    super.key,
    required this.chatId,
    required this.memberId,
  });

  final ChatId chatId;
  final UiUserId memberId;

  @override
  Widget build(BuildContext context) {
    return MultiBlocProvider(
      providers: [
        BlocProvider(
          create: (context) => ChatDetailsCubit(
            userCubit: context.read<UserCubit>(),
            userSettingsCubit: context.read<UserSettingsCubit>(),
            attachmentsRepository: context.read<AttachmentsRepository>(),
            chatId: chatId,
            chat: context.read<ChatsRepository>().getChat(chatId),
          ),
        ),
        BlocProvider(
          create: (context) => MemberDetailsCubit(
            userCubit: context.read<UserCubit>(),
            chatId: chatId,
          ),
        ),
      ],
      child: Builder(
        builder: (context) {
          final profile = context.select(
            (UsersCubit cubit) => cubit.state.profile(userId: memberId),
          );

          final groupTitle = context.select(
            (ChatDetailsCubit cubit) => cubit.state.chat?.title,
          );

          final canKick = context.select(
            (MemberDetailsCubit cubit) =>
                cubit.state.roomState?.canKick(target: memberId) ?? false,
          );

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
                    relationship: MemberRelationship(
                      groupChatId: chatId,
                      groupTitle: groupTitle ?? "",
                      canKick: canKick,
                    ),
                  ),
          );
        },
      ),
    );
  }
}

/// Closes the topmost modal. The navigation state owns the stack, so the
/// header's actions go through it rather than through the navigator.
void _pop(BuildContext context) => context.read<NavigationCubit>().pop();
