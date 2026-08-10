// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat_details/mute_button.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/list_group/list_group.dart';
import 'package:air/ds/components/list_group/list_group_tokens.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/features/user/avatar.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';

import 'package:air/features/chat_details/change_group_title_dialog.dart';
import 'package:air/features/developer/chat_debug_info_view.dart'
    show showChatDebugInfo;
import 'package:air/features/chat/chat_details_cubit.dart';

/// Body of the group details modal page. Content only: surface, header, and
/// scrolling are the modal's.
class GroupDetailsView extends StatelessWidget {
  const GroupDetailsView({super.key});

  @override
  Widget build(BuildContext context) {
    final (chat, members) = context.select((ChatDetailsCubit cubit) {
      final state = cubit.state;
      return (state.chat, state.members);
    });

    if (chat == null) {
      return const SizedBox.shrink();
    }

    return ModalBody(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Center(
            child: ChatAvatar(
              chatId: chat.id,
              size: 192,
              onPressed: () => _selectAvatar(context, chat.id),
            ),
          ),

          const SizedBox(height: S.s16),

          _GroupTitle(chat: chat),

          const SizedBox(height: S.s24),

          const Center(child: MuteButton()),

          const SizedBox(height: S.s24),

          _PeopleSection(memberIds: members),

          const SizedBox(height: S.s24),

          _GroupActions(chat: chat),
        ],
      ),
    );
  }

  void _selectAvatar(BuildContext context, ChatId id) async {
    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    final ImagePicker picker = ImagePicker();
    // Reduce image quality to re-encode the image.
    final XFile? image = await picker.pickImage(
      source: ImageSource.gallery,
      imageQuality: 99,
    );
    if (image == null) {
      return;
    }
    final bytes = await image.readAsBytes();
    chatDetailsCubit.setChatPicture(bytes: bytes);
  }
}

class _GroupTitle extends StatelessWidget {
  const _GroupTitle({required this.chat});

  final UiChatDetails chat;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: () => _changeGroupTitle(context),
      onLongPress: () => showChatDebugInfo(context, chat),
      child: Text(
        chat.title,
        textAlign: TextAlign.center,
        style: typeScale.header.xl.style(weight: Weight.emphasized),
      ),
    );
  }

  void _changeGroupTitle(BuildContext context) {
    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    showDialog(
      context: context,
      builder: (context) => BlocProvider<ChatDetailsCubit>.value(
        value: chatDetailsCubit,
        child: ChangeGroupTitleDialog(groupTitle: chat.title),
      ),
    );
  }
}

/// How many members the section previews before the full list takes over.
const _previewMemberCount = 8;

/// The member count, a way into the full list, and the first few members.
class _PeopleSection extends HookWidget {
  const _PeopleSection({required this.memberIds});

  final List<UiUserId> memberIds;

  @override
  Widget build(BuildContext context) {
    final profiles = context.select(
      (UsersCubit cubit) => {
        for (final userId in memberIds)
          userId: cubit.state.profile(userId: userId),
      },
    );

    // Sorted the way the full list sorts, so the preview reads as its head.
    final previewIds = useMemoized(
      () => memberIds
          .sortedBy((userId) => profiles[userId]!.displayName.toLowerCase())
          .take(_previewMemberCount)
          .toList(),
      [memberIds, profiles],
    );

    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);
    final rowTokens = ListRowTokens.current;

    void openGroupMembers() =>
        context.read<NavigationCubit>().openGroupMembers();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ListRow(
          tokens: rowTokens,
          label: loc.groupDetails_memberCount(memberIds.length),
          labelStyle: typeScale.body.regular.style(weight: Weight.emphasized),
          separator: false,
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                loc.groupDetails_seeAll,
                style: typeScale.body.regular.style(
                  color: palette.text.secondary,
                  tight: true,
                ),
              ),
              const SizedBox(width: S.s8),
              ButtonIcon(
                variant: ButtonIconVariant.solid,
                size: ButtonIconSize.s32,
                icon: AppIconType.arrowRight,
                fill: palette.backgroundBase.secondary,
                iconColor: palette.text.secondary,
                onPressed: openGroupMembers,
              ),
            ],
          ),
          onTap: openGroupMembers,
        ),

        ListGroup(
          tokens: ListGroupTokens.current,
          color: ListGroup.noFill,
          radius: CornerRadius.px12,
          children: [
            _AddPeopleRow(tokens: rowTokens),
            for (final memberId in previewIds) ...[
              // The group has no fill, so a hairline gap separates the rows.
              const SizedBox(height: StrokeWidth.px1),
              _MemberRow(tokens: rowTokens, memberId: memberId),
            ],
          ],
        ),
      ],
    );
  }
}

class _AddPeopleRow extends StatelessWidget {
  const _AddPeopleRow({required this.tokens});

  final ListRowTokens tokens;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);
    void openAddMembers() => context.read<NavigationCubit>().openAddMembers();

    return ListRow(
      tokens: tokens,
      fill: palette.backgroundBase.secondary,
      radius: CornerRadius.px0,
      label: loc.groupDetails_addPeople,
      leading: ButtonIcon(
        variant: ButtonIconVariant.solid,
        size: ButtonIconSize.s32,
        icon: AppIconType.plus,
        fill: palette.function.neutral.toggleWhite,
        iconColor: palette.text.primary,
        onPressed: openAddMembers,
      ),
      onTap: openAddMembers,
    );
  }
}

class _MemberRow extends StatelessWidget {
  const _MemberRow({required this.tokens, required this.memberId});

  final ListRowTokens tokens;
  final UiUserId memberId;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: memberId),
    );
    final ownUserId = context.select((UserCubit cubit) => cubit.state.userId);

    final isSelf = memberId == ownUserId;
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    return ListRow(
      tokens: tokens,
      fill: palette.backgroundBase.secondary,
      radius: CornerRadius.px0,
      label: isSelf ? loc.chatList_you : profile.displayName,
      leading: UserAvatar(profile: profile, size: S.s32),
      onTap: isSelf
          ? null
          : () => context.read<NavigationCubit>().openMemberDetails(memberId),
    );
  }
}

/// Leaving and deleting, the two ways out of a group.
class _GroupActions extends StatelessWidget {
  const _GroupActions({required this.chat});

  final UiChatDetails chat;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final size = ButtonSize.current;

    return Row(
      children: [
        Expanded(
          child: Button(
            onPressed: () => _leave(context),
            size: size,
            type: ButtonType.secondary,
            label: loc.groupDetails_leaveChat,
          ),
        ),

        const SizedBox(width: S.s12),

        Expanded(
          child: Button(
            onPressed: () => _delete(context),
            size: size,
            type: ButtonType.secondary,
            tone: ButtonTone.danger,
            label: loc.groupDetails_deleteChat,
          ),
        ),
      ],
    );
  }

  void _leave(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    final loc = AppLocalizations.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ConfirmDialog(
        title: loc.leaveChatDialog_title,
        message: loc.leaveChatDialog_content(chat.title),
        cancel: loc.leaveChatDialog_cancel,
        confirm: loc.leaveChatDialog_leave,
      ),
    );
    if (confirmed ?? false) {
      userCubit.leaveChat(chat.id);
      navigationCubit.closeChat();
    }
  }

  void _delete(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    final loc = AppLocalizations.of(context);

    final confirmed = await showAdaptiveConfirm(
      context: context,
      title: loc.deleteChatDialog_title,
      description: loc.deleteChatDialog_content,
      primaryActionText: loc.deleteChatDialog_delete,
      primaryTone: ButtonTone.danger,
    );
    if (!confirmed) return;
    AppHaptics.destructive();
    userCubit.deleteChat(chat.id);
    if (!context.mounted) return;
    navigationCubit.closeChat();
  }
}
