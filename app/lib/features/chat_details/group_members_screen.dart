// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/member_details_cubit.dart';
import 'package:air/features/chat_details/member_list_item.dart';
import 'package:air/features/chat_details/member_search_field.dart';
import 'package:air/features/chat_details/remove_member_button.dart';

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
      child: const GroupMembersView(),
    );
  }
}

class GroupMembersView extends HookWidget {
  const GroupMembersView({super.key});

  @override
  Widget build(BuildContext context) {
    final chatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );

    if (chatId == null) {
      return const SizedBox.shrink();
    }

    final members = context.select(
      (ChatDetailsCubit cubit) => cubit.state.members,
    );

    final profiles = context.select(
      (UsersCubit cubit) => {
        for (final userId in members)
          userId: cubit.state.profile(userId: userId),
      },
    );
    final roomState = context.select(
      (MemberDetailsCubit cubit) => cubit.state.roomState,
    );
    final ownUserId = context.select((UserCubit cubit) => cubit.state.userId);

    final controller = useTextEditingController();
    final query = useState("");

    final loc = AppLocalizations.of(context);

    final sortedMembers = useMemoized(() {
      final youValue = loc.chatList_you.toLowerCase();
      final filteredMembers = members.where((memberId) {
        if (query.value.isEmpty) return true;
        final name = profiles[memberId]!.displayName.toLowerCase();
        if (name.contains(query.value)) return true;
        return memberId == ownUserId && youValue.contains(query.value);
      });
      return filteredMembers.sortedBy(
        (userId) => profiles[userId]!.displayName.toLowerCase(),
      );
    }, [members, profiles, query, ownUserId]);

    return ModalScaffold(
      title: loc.groupMembersScreen_title,
      onLeading: () => context.read<NavigationCubit>().pop(),
      trailing: const _AddMembersAction(),
      // The list below the search field scrolls on its own.
      scrollable: false,
      child: Column(
        children: [
          MemberSearchField(
            controller: controller,
            hintText: loc.groupMembersScreen_searchHint,
            onChanged: (value) => query.value = value.toLowerCase().trim(),
          ),
          Expanded(
            child: AppScrollbar(
              child: ScrollConfiguration(
                behavior: ScrollConfiguration.of(
                  context,
                ).copyWith(scrollbars: false),
                child: _MemberList(
                  chatId: chatId,
                  members: sortedMembers,
                  ownUserId: ownUserId,
                  roomState: roomState,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _AddMembersAction extends StatelessWidget {
  const _AddMembersAction();

  @override
  Widget build(BuildContext context) {
    return DialogHeaderAction(
      tokens: DialogHeaderTokens.of(context),
      icon: AppIconType.plus,
      onPressed: () => context.read<NavigationCubit>().openAddMembers(),
    );
  }
}

class _MemberList extends StatelessWidget {
  const _MemberList({
    required this.chatId,
    required this.members,
    required this.ownUserId,
    required this.roomState,
  });

  final ChatId chatId;
  final List<UiUserId> members;
  final UiUserId ownUserId;
  final UiRoomState? roomState;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return ListView.separated(
      // The modal surface runs to the bottom of the screen, so the last row
      // clears the home indicator on its own rather than through a SafeArea.
      padding: EdgeInsets.fromLTRB(
        S.s16,
        S.s12,
        S.s16,
        S.s12 + MediaQuery.viewPaddingOf(context).bottom,
      ),
      itemCount: members.length,
      separatorBuilder: (context, index) => Divider(
        height: 1,
        thickness: StrokeWidth.px1,
        color: palette.backgroundBase.primary,
      ),
      itemBuilder: (context, index) => _GroupMemberTile(
        chatId: chatId,
        memberId: members[index],
        ownUserId: ownUserId,
        roomState: roomState,
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
      onTap: isSelf
          ? null
          : () => context.read<NavigationCubit>().openMemberDetails(memberId),
      trailing: isSelf
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
