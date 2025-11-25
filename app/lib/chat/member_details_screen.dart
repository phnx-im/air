// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'member_details_cubit.dart';
import 'report_spam_button.dart';
import 'widgets/remove_member_button.dart';

class MemberDetailsScreen extends StatelessWidget {
  const MemberDetailsScreen({super.key});

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
        return MemberDetailsCubit(
          userCubit: context.read<UserCubit>(),
          chatId: chatId,
        );
      },
      child: const MemberDetailsScreenView(),
    );
  }
}

class MemberDetailsScreenView extends StatelessWidget {
  const MemberDetailsScreenView({super.key});

  @override
  Widget build(BuildContext context) {
    final (chatId, memberId) = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        NavigationState_Home(
          home: HomeNavigationState(
            :final currentChat,
            memberDetails: final memberId,
          ),
        ) =>
          (currentChat.chatId, memberId),
        _ => (null, null),
      },
    );
    if (chatId == null || memberId == null) {
      return const SizedBox.shrink();
    }

    final ownUserId = context.select((UserCubit cubit) => cubit.state.userId);
    final isSelf = memberId == ownUserId;

    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: memberId),
    );

    final roomState = context.select(
      (MemberDetailsCubit cubit) => cubit.state.roomState,
    );
    if (roomState == null) {
      return const SizedBox.shrink();
    }

    final canKick = roomState.canKick(target: memberId);

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        leading: const AppBarBackButton(),
        title: Text(profile.displayName),
      ),
      body: SafeArea(
        child: MemberDetailsView(
          chatId: chatId,
          profile: profile,
          isSelf: isSelf,
          canKick: canKick,
        ),
      ),
    );
  }
}

/// Details of a member of a chat
class MemberDetailsView extends StatelessWidget {
  const MemberDetailsView({
    required this.chatId,
    required this.profile,
    required this.isSelf,
    required this.canKick,
    super.key,
  });

  final ChatId chatId;
  final UiUserProfile profile;
  final bool isSelf;
  final bool canKick;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: Spacings.l),
        child: Column(
          children: [
            const SizedBox(height: Spacings.l),
            UserAvatar(
              size: 192,
              displayName: profile.displayName,
              image: profile.profilePicture,
            ),
            const SizedBox(height: Spacings.l),
            Text(
              style: Theme.of(
                context,
              ).textTheme.displayLarge!.copyWith(fontWeight: FontWeight.bold),
              profile.displayName,
            ),

            const Spacer(),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              spacing: Spacings.m,
              children: [
                // Show the remove user button if the user is not the current user and has kicking rights
                if (!isSelf && canKick)
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.only(bottom: Spacings.s),
                      child: RemoveMemberButton(
                        chatId: chatId,
                        memberId: profile.userId,
                        displayName: profile.displayName,
                        onRemoved: () {
                          if (Navigator.of(context).canPop()) {
                            Navigator.of(context).pop();
                          }
                        },
                      ),
                    ),
                  ),
                if (!isSelf)
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.only(bottom: Spacings.s),
                      child: ReportSpamButton(userId: profile.userId),
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
