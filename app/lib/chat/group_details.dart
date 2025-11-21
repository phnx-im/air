// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/widgets/member_list_item.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
import 'package:air/user/user.dart';
import 'package:air/util/dialog.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:image_picker/image_picker.dart';
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
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.symmetric(vertical: Spacings.m),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    UserAvatar(
                      size: 192,
                      image: chat.picture,
                      displayName: chat.title,
                      onPressed: () => _selectAvatar(context, chat.id),
                    ),
                    const SizedBox(height: Spacings.m),
                    Text(
                      chat.title,
                      textAlign: TextAlign.center,
                      style: Theme.of(context).textTheme.displayLarge!.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(height: Spacings.m),
                    Text(
                      chat.chatType.description,
                      textAlign: TextAlign.center,
                      style: Theme.of(context).textTheme.bodyMedium,
                    ),
                    const SizedBox(height: Spacings.m),
                    _PeoplePreview(
                      memberIds: members,
                      onOpenPressed: () {
                        context.read<NavigationCubit>().openGroupMembers();
                      },
                    ),
                  ],
                ),
              ),
            ),
            Padding(
              padding: EdgeInsets.symmetric(
                horizontal: Spacings.s,
                vertical: isPointer() ? Spacings.s : Spacings.xxxs,
              ),
              child: Row(
                children: [
                  Expanded(
                    child: OutlinedButton(
                      onPressed: () => _leave(context, chat),
                      child: Text(
                        loc.groupDetails_leaveChat,
                        style: Theme.of(context).textTheme.bodyMedium!,
                      ),
                    ),
                  ),
                  const SizedBox(width: Spacings.xs),
                  Expanded(
                    child: OutlinedButton(
                      style: OutlinedButton.styleFrom(
                        backgroundColor: CustomColorScheme.of(
                          context,
                        ).function.danger,
                        foregroundColor: CustomColorScheme.of(
                          context,
                        ).function.white,
                      ),
                      onPressed: () => _delete(context, chat),
                      child: Text(
                        loc.groupDetails_deleteChat,
                        style: Theme.of(context).textTheme.bodyMedium!.copyWith(
                          color: CustomColorScheme.of(context).function.white,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
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
}

class _PeoplePreview extends StatelessWidget {
  const _PeoplePreview({required this.memberIds, this.onOpenPressed});

  final List<UiUserId> memberIds;
  final VoidCallback? onOpenPressed;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final textTheme = Theme.of(context).textTheme;
    final loc = AppLocalizations.of(context);
    final previewIds = memberIds.take(3).toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(
            left: Spacings.xs,
            bottom: Spacings.xs,
          ),
          child: Text(
            loc.groupDetails_memberCount(memberIds.length),
            style: textTheme.labelLarge!.copyWith(
              fontWeight: FontWeight.bold,
              color: colors.text.primary,
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
            _SeeAllRow(
              onPressed: onOpenPressed,
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

class _SeeAllRow extends StatelessWidget {
  const _SeeAllRow({required this.onPressed, required this.position});

  final VoidCallback? onPressed;
  final _PeopleEntryPosition position;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final textTheme = Theme.of(context).textTheme;
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
      child: InkWell(
        onTap: onPressed,
        borderRadius: borderRadius,
        child: Container(
          decoration: BoxDecoration(
            color: colors.backgroundBase.secondary,
            borderRadius: borderRadius,
          ),
          padding: const EdgeInsets.symmetric(
            horizontal: Spacings.s,
            vertical: Spacings.xs,
          ),
          child: Row(
            children: [
              Text(
                loc.groupDetails_seeAll,
                style: textTheme.bodyMedium?.copyWith(
                  color: colors.text.primary,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const Spacer(),
              if (onPressed != null)
                iconoir.ArrowRight(width: 16, color: colors.text.primary),
            ],
          ),
        ),
      ),
    );
  }
}
