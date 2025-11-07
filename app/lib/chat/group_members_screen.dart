// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'chat_details_cubit.dart';
import 'member_details_cubit.dart';
import 'widgets/member_list_item.dart';
import 'widgets/member_search_field.dart';
import 'widgets/remove_member_button.dart';

class GroupMembersScreen extends StatelessWidget {
  const GroupMembersScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final chatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );

    if (chatId == null) {
      return const SizedBox.shrink();
    }

    return MultiBlocProvider(
      providers: [
        BlocProvider(
          create:
              (context) => ChatDetailsCubit(
                userCubit: context.read<UserCubit>(),
                userSettingsCubit: context.read<UserSettingsCubit>(),
                chatsRepository: context.read<ChatsRepository>(),
                chatId: chatId,
              ),
        ),
        BlocProvider(
          create:
              (context) => MemberDetailsCubit(
                userCubit: context.read<UserCubit>(),
                chatId: chatId,
              ),
        ),
      ],
      child: const _GroupMembersView(),
    );
  }
}

class _GroupMembersView extends StatefulWidget {
  const _GroupMembersView();

  @override
  State<_GroupMembersView> createState() => _GroupMembersViewState();
}

class _GroupMembersViewState extends State<_GroupMembersView> {
  final TextEditingController _controller = TextEditingController();
  String _query = '';

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final members = context.select(
      (ChatDetailsCubit cubit) => cubit.state.members,
    );
    final colorScheme = CustomColorScheme.of(context);
    final chatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );
    final roomState = context.select(
      (MemberDetailsCubit cubit) => cubit.state.roomState,
    );
    final ownUserId = context.select((UserCubit cubit) => cubit.state.userId);
    final usersState = context.select((UsersCubit cubit) => cubit.state);

    if (chatId == null) {
      return const SizedBox.shrink();
    }

    final query = _query.trim().toLowerCase();
    final filteredMembers =
        members.where((memberId) {
          if (query.isEmpty) return true;
          final name = usersState.displayName(userId: memberId).toLowerCase();
          if (name.contains(query)) return true;
          if (memberId == ownUserId &&
              loc.chatList_you.toLowerCase().contains(query)) {
            return true;
          }
          return false;
        }).toList();

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        leading: const AppBarBackButton(),
        title: Text(loc.groupMembersScreen_title),
        actions: [
          AppBarPlusButton(
            onPressed: () => context.read<NavigationCubit>().openAddMembers(),
          ),
        ],
      ),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: Container(
            constraints:
                isPointer() ? const BoxConstraints(maxWidth: 800) : null,
            child: Column(
              children: [
                MemberSearchField(
                  controller: _controller,
                  hintText: loc.groupMembersScreen_searchHint,
                  onChanged: (value) => setState(() => _query = value),
                ),
                Expanded(
                  child: ListView.separated(
                    padding: const EdgeInsets.symmetric(
                      horizontal: Spacings.m,
                      vertical: Spacings.xs,
                    ),
                    itemCount: filteredMembers.length,
                    separatorBuilder:
                        (context, index) => Divider(
                          height: 1,
                          thickness: 1,
                          color: colorScheme.backgroundBase.primary,
                        ),
                    itemBuilder:
                        (context, index) => _GroupMemberTile(
                          chatId: chatId,
                          memberId: filteredMembers[index],
                          ownUserId: ownUserId,
                          roomState: roomState,
                        ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _GroupMemberTile extends StatelessWidget {
  const _GroupMemberTile({
    required this.chatId,
    required this.memberId,
    required this.ownUserId,
    required this.roomState,
  });

  final ChatId chatId;
  final UiUserId memberId;
  final UiUserId ownUserId;
  final UiRoomState? roomState;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: memberId),
    );
    final canKick = roomState?.canKick(target: memberId) ?? false;
    final isSelf = memberId == ownUserId;
    final loc = AppLocalizations.of(context);
    final displayName = isSelf ? loc.chatList_you : profile.displayName;

    return MemberListItem(
      profile: profile,
      displayNameOverride: displayName,
      enabled: !isSelf,
      onTap:
          isSelf
              ? null
              : () =>
                  context.read<NavigationCubit>().openMemberDetails(memberId),
      trailing:
          isSelf
              ? null
              : RemoveMemberButton(
                chatId: chatId,
                memberId: memberId,
                displayName: profile.displayName,
                compact: true,
                enabled: canKick,
              ),
    );
  }
}
