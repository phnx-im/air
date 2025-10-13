// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/chat_details/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:provider/provider.dart';

import 'report_spam_button.dart';

class MemberDetailsScreen extends StatelessWidget {
  const MemberDetailsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final (chatId, memberId) = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        NavigationState_Home(
          home: HomeNavigationState(
            chatId: final chatId,
            memberDetails: final memberId,
          ),
        ) =>
          (chatId, memberId),
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
      (ChatDetailsCubit cubit) => cubit.state.roomState,
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
        title: Text(loc.memberDetailsScreen_title),
      ),
      body: MemberDetails(
        chatId: chatId,
        profile: profile,
        isSelf: isSelf,
        canKick: canKick,
      ),
    );
  }
}

/// Details of a member of a chat
class MemberDetails extends StatelessWidget {
  const MemberDetails({
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

          const Spacer(),

          // Show the remove user button if the user is not the current user and has kicking rights
          if (!isSelf && canKick)
            Padding(
              padding: const EdgeInsets.only(bottom: Spacings.s),
              child: _RemoveUserButton(
                chatId: chatId,
                userId: profile.userId,
                displayName: profile.displayName,
              ),
            ),

          if (!isSelf)
            Padding(
              padding: const EdgeInsets.only(bottom: Spacings.s),
              child: ReportSpamButton(userId: profile.userId),
            ),
        ],
      ),
    );
  }
}

class _RemoveUserButton extends StatelessWidget {
  const _RemoveUserButton({
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

    return OutlinedButton(
      onPressed: () => _onPressed(context),
      child: Text(loc.removeUserButton_text),
    );
  }

  void _onPressed(BuildContext context) async {
    bool confirmed = await showDialog(
      context: context,
      builder: (BuildContext context) {
        final loc = AppLocalizations.of(context);

        return AlertDialog(
          title: Text(loc.removeUserDialog_title),
          content: Text(loc.removeUserDialog_content(displayName)),
          actions: [
            TextButton(
              onPressed: () {
                Navigator.of(context).pop(false);
              },
              child: Text(loc.removeUserDialog_cancel),
            ),
            TextButton(
              onPressed: () async {
                await context.read<UserCubit>().removeUserFromChat(
                  chatId,
                  userId,
                );
                if (context.mounted) {
                  Navigator.of(context).pop(true);
                }
              },
              child: Text(loc.removeUserDialog_removeUser),
            ),
          ],
        );
      },
    );
    if (confirmed && context.mounted) {
      Navigator.of(context).pop(true);
    }
  }
}
