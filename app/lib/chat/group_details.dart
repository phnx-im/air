// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/colors/themes.dart';
import 'package:air/util/dialog.dart';
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/core/core.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:provider/provider.dart';

import 'chat_details_cubit.dart';

/// Details of a group chat
class GroupDetails extends StatelessWidget {
  const GroupDetails({super.key});

  @override
  Widget build(BuildContext context) {
    final (chat, members) = context.select((ChatDetailsCubit cubit) {
      final state = cubit.state;
      return (state.chat, state.members);
    });

    if (chat == null) {
      return const SizedBox.shrink();
    }

    final loc = AppLocalizations.of(context);

    return Align(
      alignment: Alignment.topCenter,
      child: Container(
        constraints: isPointer() ? const BoxConstraints(maxWidth: 800) : null,
        padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
        child: Column(
          children: [
            const SizedBox(height: Spacings.l),
            UserAvatar(
              size: 128,
              image: chat.picture,
              displayName: chat.title,
              onPressed: () => _selectAvatar(context, chat.id),
            ),
            const SizedBox(height: Spacings.l),
            Text(chat.title, style: Theme.of(context).textTheme.bodyLarge),
            const SizedBox(height: Spacings.l),
            Text(
              chat.chatType.description,
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            const SizedBox(height: Spacings.l),

            Expanded(
              child: Container(
                constraints: const BoxConstraints(minWidth: 100, maxWidth: 600),
                child: ListView.builder(
                  shrinkWrap: true,
                  itemCount: members.length,
                  itemBuilder: (context, index) {
                    if (index == 0) {
                      return Column(
                        children: [
                          // Header
                          Text(
                            loc.groupDetails_members,
                            style: Theme.of(context).textTheme.labelLarge,
                          ),
                          _MemberTile(memberId: members[index]),
                        ],
                      );
                    } else {
                      return _MemberTile(memberId: members[index]);
                    }
                  },
                ),
              ),
            ),
            const SizedBox(height: Spacings.l),

            OutlinedButton(
              onPressed: () {
                context.read<NavigationCubit>().openAddMembers();
              },
              child: Text(loc.groupDetails_addMembers),
            ),
            const SizedBox(height: Spacings.s),

            OutlinedButton(
              onPressed: () => _leave(context, chat),
              child: Text(
                loc.groupDetails_leaveChat,
                style: TextStyle(
                  color: CustomColorScheme.of(context).function.danger,
                ),
              ),
            ),
            const SizedBox(height: Spacings.s),

            OutlinedButton(
              onPressed: () => _delete(context, chat.id),
              child: Text(
                loc.groupDetails_deleteChat,
                style: TextStyle(
                  color: CustomColorScheme.of(context).function.danger,
                ),
              ),
            ),
            const SizedBox(height: Spacings.s),
          ],
        ),
      ),
    );
  }

  void _selectAvatar(BuildContext context, ChatId id) async {
    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    final ImagePicker picker = ImagePicker();
    final XFile? image = await picker.pickImage(source: ImageSource.gallery);
    if (image == null) {
      return;
    }
    final bytes = await image.readAsBytes();
    chatDetailsCubit.setChatPicture(bytes: bytes);
  }

  void _leave(BuildContext context, UiChatDetails chatDetails) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    final loc = AppLocalizations.of(context);
    if (await showConfirmationDialog(
      context,
      title: loc.leaveChatDialog_title,
      message: loc.leaveChatDialog_content(chatDetails.title),
      positiveButtonText: loc.leaveChatDialog_leave,
      negativeButtonText: loc.leaveChatDialog_cancel,
    )) {
      userCubit.leaveChat(chatDetails.id);
      navigationCubit.closeChat();
    }
  }

  void _delete(BuildContext context, ChatId id) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    final loc = AppLocalizations.of(context);
    if (await showConfirmationDialog(
      context,
      title: loc.deleteChatDialog_title,
      message: loc.deleteChatDialog_content,
      positiveButtonText: loc.deleteChatDialog_delete,
      negativeButtonText: loc.deleteChatDialog_cancel,
    )) {
      userCubit.deleteChat(id);
      navigationCubit.closeChat();
    }
  }
}

class _MemberTile extends StatelessWidget {
  const _MemberTile({required this.memberId});

  final UiUserId memberId;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: memberId),
    );

    return ListTile(
      leading: UserAvatar(
        displayName: profile.displayName,
        image: profile.profilePicture,
        size: Spacings.l,
      ),
      title: Text(
        profile.displayName,
        style: Theme.of(context).textTheme.bodyMedium,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: const Icon(Icons.more_horiz),
      onTap: () {
        context.read<NavigationCubit>().openMemberDetails(memberId);
      },
    );
  }
}
