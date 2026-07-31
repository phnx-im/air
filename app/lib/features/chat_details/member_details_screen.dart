// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat_details/contact_details_view.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
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

          return AppScaffold(
            title: groupTitle ?? "",
            child: ContactDetailsView(
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
