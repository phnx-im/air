// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/widgets/member_list_item.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/ui/components/desktop/width_constraints.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/util/dialog.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:air/ui/icons/app_icons.dart';

import 'change_group_title_dialog.dart';
import 'chat_details_cubit.dart';

/// Details of a group chat
class GroupDetailsScreen extends StatelessWidget {
  const GroupDetailsScreen({super.key});

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
    final colors = CustomColorScheme.of(context);

    return AppScaffold(
      title: chat.title,
      child: Align(
        alignment: Alignment.topCenter,
        child: ConstrainedWidth(
          child: Column(
            children: [
              Expanded(
                child: SingleChildScrollView(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.center,
                    children: [
                      const SizedBox(height: Spacings.s),
                      ChatAvatar(
                        chatId: chat.id,
                        size: 192,
                        onPressed: () => _selectAvatar(context, chat.id),
                      ),
                      const SizedBox(height: Spacings.s),
                      InkWell(
                        onTap: () => _changeGroupTitle(context, chat.title),
                        child: Text(
                          chat.title,
                          textAlign: TextAlign.center,
                          style: TextStyle(
                            fontSize: HeaderFontSize.h1.size,
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ),
                      const SizedBox(height: Spacings.xxs),
                      Text(
                        loc.groupDetails_groupDescription,
                        textAlign: TextAlign.center,
                        style: TextStyle(
                          fontSize: BodyFontSize.base.size,
                          color: colors.text.secondary,
                        ),
                      ),
                      const SizedBox(height: Spacings.l),
                      _PeoplePreview(memberIds: members),
                    ],
                  ),
                ),
              ),
              Row(
                children: [
                  Expanded(
                    child: AppButton(
                      onPressed: () => _leave(context, chat),
                      type: .secondary,
                      label: loc.groupDetails_leaveChat,
                    ),
                  ),
                  const SizedBox(width: Spacings.xs),
                  Expanded(
                    child: AppButton(
                      onPressed: () => _delete(context, chat),
                      tone: .danger,
                      label: loc.groupDetails_deleteChat,
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
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

  void _delete(BuildContext context, UiChatDetails chat) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    final loc = AppLocalizations.of(context);

    final confirmed =
        await showBottomSheetModal<bool>(
          context: context,
          builder: (sheetContext) {
            return BottomSheetDialogContent(
              title: loc.deleteChatDialog_title,
              description: loc.deleteChatDialog_content,
              primaryActionText: loc.deleteChatDialog_delete,
              isPrimaryDanger: true,
            );
          },
        ) ??
        false;
    if (!confirmed) return;
    userCubit.deleteChat(chat.id);
    if (!context.mounted) return;
    navigationCubit.closeChat();
  }

  void _changeGroupTitle(BuildContext context, String chatTitle) {
    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    showDialog(
      context: context,
      builder: (context) => BlocProvider<ChatDetailsCubit>.value(
        value: chatDetailsCubit,
        child: ChangeGroupTitleDialog(groupTitle: chatTitle),
      ),
    );
  }
}

class _PeoplePreview extends HookWidget {
  const _PeoplePreview({required this.memberIds});

  final List<UiUserId> memberIds;

  @override
  Widget build(BuildContext context) {
    final profiles = context.select(
      (UsersCubit cubit) => {
        for (final userId in memberIds)
          userId: cubit.state.profile(userId: userId),
      },
    );

    final previewIds = useMemoized(
      () => top3(
        memberIds,
        (userId) => profiles[userId]!.displayName.toLowerCase(),
      ),
      [memberIds, profiles],
    );

    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.all(Spacings.xs),
          child: Text(
            loc.groupDetails_memberCount(memberIds.length),
            style: TextStyle(
              fontSize: LabelFontSize.base.size,
              fontWeight: FontWeight.bold,
            ),
          ),
        ),
        Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            for (var i = 0; i < previewIds.length; i++) ...[
              _PeoplePreviewEntry(
                memberId: previewIds[i],
                position: i == 0
                    ? _PeopleEntryPosition.first
                    : _PeopleEntryPosition.middle,
              ),
              if (i < previewIds.length)
                Divider(
                  height: 1,
                  thickness: 1,
                  color: colors.backgroundBase.primary,
                ),
            ],
            _ActionsRow(
              position: previewIds.isEmpty
                  ? _PeopleEntryPosition.single
                  : _PeopleEntryPosition.last,
            ),
          ],
        ),
      ],
    );
  }
}

enum _PeopleEntryPosition { single, first, middle, last }

class _PeoplePreviewEntry extends StatelessWidget {
  const _PeoplePreviewEntry({required this.memberId, required this.position});

  final UiUserId memberId;
  final _PeopleEntryPosition position;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: memberId),
    );
    final ownUserId = context.select((UserCubit cubit) => cubit.state.userId);

    final isSelf = memberId == ownUserId;
    final loc = AppLocalizations.of(context);
    final displayName = isSelf ? loc.chatList_you : profile.displayName;

    final colors = CustomColorScheme.of(context);

    final borderRadius = switch (position) {
      _PeopleEntryPosition.single => BorderRadius.circular(16),
      _PeopleEntryPosition.first => const BorderRadius.vertical(
        top: Radius.circular(16),
      ),
      _PeopleEntryPosition.middle => BorderRadius.zero,
      _PeopleEntryPosition.last => BorderRadius.zero,
    };

    return Container(
      decoration: BoxDecoration(
        color: colors.backgroundBase.secondary,
        borderRadius: borderRadius,
      ),
      padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
      child: MemberListItem(
        profile: profile,
        displayNameOverride: displayName,
        enabled: !isSelf,
        onTap: isSelf
            ? null
            : () => context.read<NavigationCubit>().openMemberDetails(memberId),
      ),
    );
  }
}

class _ActionsRow extends StatelessWidget {
  const _ActionsRow({required this.position});

  final _PeopleEntryPosition position;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);
    final borderRadius = switch (position) {
      _PeopleEntryPosition.single => BorderRadius.circular(16),
      _PeopleEntryPosition.first => const BorderRadius.vertical(
        top: Radius.circular(16),
      ),
      _PeopleEntryPosition.last => const BorderRadius.vertical(
        bottom: Radius.circular(16),
      ),
      _PeopleEntryPosition.middle => BorderRadius.zero,
    };

    return Material(
      color: Colors.transparent,
      child: Container(
        decoration: BoxDecoration(
          color: colors.backgroundBase.secondary,
          borderRadius: borderRadius,
        ),
        child: Row(
          children: [
            Expanded(
              child: InkWell(
                onTap: () {
                  context.read<NavigationCubit>().openAddMembers();
                },
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: Spacings.s,
                    vertical: Spacings.xs,
                  ),
                  child: Row(
                    children: [
                      Container(
                        height: 32,
                        width: 32,
                        decoration: BoxDecoration(
                          color: colors.backgroundElevated.primary,
                          borderRadius: BorderRadius.circular(16),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.all(Spacings.xxs),
                          child: AppIcon.plus(
                            size: 16,
                            color: colors.function.toggleBlack,
                          ),
                        ),
                      ),
                      const SizedBox(width: Spacings.s),
                      Text(
                        loc.groupDetails_addPeople,
                        style: TextStyle(fontSize: BodyFontSize.base.size),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            Expanded(
              child: InkWell(
                onTap: () {
                  context.read<NavigationCubit>().openGroupMembers();
                },
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: Spacings.s,
                    vertical: Spacings.xs,
                  ),
                  child: Row(
                    mainAxisAlignment: .end,
                    children: [
                      Text(
                        loc.groupDetails_seeAll,
                        style: TextStyle(fontSize: BodyFontSize.base.size),
                      ),
                      const SizedBox(width: Spacings.xs),
                      Container(
                        height: 32,
                        width: 32,
                        decoration: BoxDecoration(
                          color: colors.backgroundElevated.primary,
                          borderRadius: BorderRadius.circular(16),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.all(Spacings.xxs),
                          child: AppIcon.arrowRight(
                            size: 16,
                            color: colors.function.toggleBlack,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

List<UiUserId> top3(List<UiUserId> list, String Function(UiUserId) keyOf) {
  UiUserId? a, b, c;

  for (var userId in list) {
    if (a == null || keyOf(userId).compareTo(keyOf(a)) < 0) {
      c = b;
      b = a;
      a = userId;
    } else if (b == null || keyOf(userId).compareTo(keyOf(b)) < 0) {
      c = b;
      b = userId;
    } else if (c == null || keyOf(userId).compareTo(keyOf(c)) < 0) {
      c = userId;
    }
  }

  return [a, b, c].whereType<UiUserId>().toList();
}
