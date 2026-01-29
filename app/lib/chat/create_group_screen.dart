// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/chat/widgets/app_bar_button.dart';
import 'package:air/chat/widgets/member_selection_list.dart';
import 'package:air/chat/widgets/member_search_field.dart';
import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/main.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/avatar.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';

import 'add_members_cubit.dart';

final _log = Logger('CreateGroupScreen');

class CreateGroupScreen extends StatelessWidget {
  const CreateGroupScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return MultiBlocProvider(
      providers: [
        BlocProvider(
          create: (context) =>
              ChatListCubit(userCubit: context.read<UserCubit>()),
        ),
        BlocProvider(
          create: (context) {
            final userCubit = context.read<UserCubit>();
            final contactsFuture = userCubit.contacts;
            return AddMembersCubit()..loadContacts(contactsFuture);
          },
        ),
      ],
      child: const _CreateGroupFlow(),
    );
  }
}

class _CreateGroupFlow extends HookWidget {
  const _CreateGroupFlow();

  @override
  Widget build(BuildContext context) {
    final showDetails = useState(false);

    return PopScope(
      canPop: !showDetails.value,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && showDetails.value) {
          showDetails.value = false;
        }
      },
      child: IndexedStack(
        index: showDetails.value ? 1 : 0,
        children: [
          _MemberSelectionStep(onNext: () => showDetails.value = true),
          _CreateGroupDetailsStep(onBack: () => showDetails.value = false),
        ],
      ),
    );
  }
}

class _MemberSelectionStep extends HookWidget {
  const _MemberSelectionStep({required this.onNext});

  final VoidCallback onNext;

  @override
  Widget build(BuildContext context) {
    final searchController = useTextEditingController();
    final query = useState('');

    final (contacts, selectedContacts) = context.select(
      (AddMembersCubit cubit) =>
          (cubit.state.contacts, cubit.state.selectedContacts),
    );
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        automaticallyImplyLeading: false,
        titleSpacing: 0,
        title: _GroupCreationAppBarTitle(
          title: loc.groupCreationScreen_title,
          leading: _CircularBackButton(
            onPressed: () => context.read<NavigationCubit>().pop(),
          ),
          trailing: AppBarButton(
            onPressed: () {
              FocusScope.of(context).unfocus();
              onNext();
            },
            child: Text(loc.groupCreationScreen_next),
          ),
        ),
      ),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: Container(
            constraints: isPointer()
                ? const BoxConstraints(maxWidth: 800)
                : null,
            child: Column(
              children: [
                MemberSearchField(
                  controller: searchController,
                  hintText: loc.groupMembersScreen_searchHint,
                  onChanged: (value) => query.value = value,
                ),
                Expanded(
                  child: MemberSelectionList(
                    contacts: contacts,
                    selectedContacts: selectedContacts,
                    query: query.value,
                    onToggle: (contact) => context
                        .read<AddMembersCubit>()
                        .toggleContact(contact.userId),
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

class _CreateGroupDetailsStep extends HookWidget {
  const _CreateGroupDetailsStep({required this.onBack});

  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final selectedIds = context.select(
      (AddMembersCubit cubit) => cubit.state.selectedContacts,
    );

    final selectedProfiles = context.select(
      (UsersCubit cubit) => {
        for (final userId in selectedIds)
          userId: cubit.state.profile(userId: userId),
      },
    );

    final sortedSelectedIds = useMemoized(
      () => selectedIds.sortedBy(
        (userId) => selectedProfiles[userId]!.displayName.toLowerCase(),
      ),
      [selectedIds, selectedProfiles],
    );

    final groupName = useState('');
    final picture = useState<Uint8List?>(null);
    final isCreating = useState(false);
    final nameFocusNode = useFocusNode();

    final isGroupNameValid = groupName.value.trim().isNotEmpty;
    final showHelperText = nameFocusNode.hasFocus && !isGroupNameValid;

    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        automaticallyImplyLeading: false,
        titleSpacing: 0,
        title: _GroupCreationAppBarTitle(
          title: loc.groupCreationDetails_title,
          leading: _CircularBackButton(
            onPressed: () => _handleBack(context, isCreating.value),
          ),
          trailing: AppBarButton(
            onPressed: isGroupNameValid && !isCreating.value
                ? () => _createGroupChat(
                    context,
                    groupName.value.trim(),
                    isCreating,
                    picture.value,
                  )
                : null,
            child: isCreating.value
                ? SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(
                      strokeWidth: 2,
                      valueColor: AlwaysStoppedAnimation<Color>(
                        colors.text.primary,
                      ),
                    ),
                  )
                : Text(loc.groupCreationDetails_create),
          ),
        ),
      ),
      body: SafeArea(
        child: GestureDetector(
          onTap: () => FocusScope.of(context).unfocus(),
          behavior: HitTestBehavior.translucent,
          child: SingleChildScrollView(
            padding: const EdgeInsets.symmetric(
              horizontal: Spacings.m,
              vertical: Spacings.m,
            ),
            child: Align(
              alignment: Alignment.topCenter,
              child: Container(
                constraints: isPointer()
                    ? const BoxConstraints(maxWidth: 800)
                    : null,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Center(
                      child: _GroupPicturePicker(
                        picture: picture.value,
                        onPick: () => _pickImage(picture),
                      ),
                    ),
                    const SizedBox(height: Spacings.l),
                    SizedBox(
                      width: double.infinity,
                      child: TextField(
                        onChanged: (value) => groupName.value = value,
                        focusNode: nameFocusNode,
                        textInputAction: TextInputAction.next,
                        textAlign: TextAlign.center,
                        style: Theme.of(context).textTheme.displayLarge
                            ?.copyWith(fontWeight: FontWeight.bold),
                        decoration: InputDecoration(
                          hintText: nameFocusNode.hasFocus
                              ? loc.groupCreationDetails_groupNameHintFocused
                              : loc.groupCreationDetails_groupNameHint,
                          hintStyle: Theme.of(context).textTheme.displayLarge
                              ?.copyWith(
                                color: colors.text.quaternary,
                                fontWeight: FontWeight.bold,
                              ),
                          border: InputBorder.none,
                          fillColor: Colors.transparent,
                          contentPadding: EdgeInsets.zero,
                        ),
                      ),
                    ),
                    if (showHelperText) ...[
                      const SizedBox(height: Spacings.xxs),
                      Center(
                        child: Text(
                          loc.groupCreationDetails_groupNameHelper,
                          textAlign: TextAlign.center,
                          style: Theme.of(context).textTheme.bodySmall
                              ?.copyWith(color: colors.text.tertiary),
                        ),
                      ),
                    ],
                    const SizedBox(height: Spacings.l),
                    if (selectedIds.isNotEmpty)
                      Wrap(
                        alignment: WrapAlignment.start,
                        spacing: Spacings.s,
                        runSpacing: Spacings.s,
                        children: sortedSelectedIds.map((userId) {
                          final profile = selectedProfiles[userId];
                          if (profile == null) {
                            return const SizedBox.shrink();
                          }
                          return _SelectedParticipant(
                            profile: profile,
                            onRemove: () => _removeContact(context, userId),
                          );
                        }).toList(),
                      )
                    else
                      Center(
                        child: Text(
                          loc.groupCreationDetails_emptySelection,
                          textAlign: TextAlign.center,
                          style: Theme.of(context).textTheme.bodyMedium
                              ?.copyWith(color: colors.text.tertiary),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _handleBack(BuildContext context, bool isCreating) {
    if (isCreating) return;
    FocusScope.of(context).unfocus();
    onBack();
  }

  void _pickImage(ValueNotifier<Uint8List?> picture) async {
    final picker = ImagePicker();
    final XFile? image = await picker.pickImage(source: ImageSource.gallery);
    if (image == null) {
      return;
    }
    final bytes = await image.readAsBytes();
    picture.value = bytes;
  }

  void _removeContact(BuildContext context, UiUserId userId) {
    context.read<AddMembersCubit>().toggleContact(userId);
  }

  Future<void> _createGroupChat(
    BuildContext context,
    String groupName,
    ValueNotifier<bool> isCreating,
    Uint8List? picture,
  ) async {
    if (groupName.isEmpty) return;
    final navigationCubit = context.read<NavigationCubit>();
    final userCubit = context.read<UserCubit>();
    final addMembersCubit = context.read<AddMembersCubit>();
    final selectedContacts = addMembersCubit.state.selectedContacts;

    isCreating.value = true;

    final chatListCubit = context.read<ChatListCubit>();

    try {
      final chatId = await chatListCubit.createGroupChat(
        groupName: groupName,
        picture: picture,
      );
      for (final userId in selectedContacts) {
        await userCubit.addUserToChat(chatId, userId);
      }
      if (!context.mounted) return;
      navigationCubit.pop();
      await navigationCubit.openChat(chatId);
    } catch (error, stackTrace) {
      _log.severe('Failed to create group "$groupName"', error, stackTrace);
      showErrorBannerStandalone((loc) => loc.newChatDialog_error(groupName));
    } finally {
      isCreating.value = false;
    }
  }
}

class _GroupPicturePicker extends StatelessWidget {
  const _GroupPicturePicker({required this.picture, required this.onPick});

  final Uint8List? picture;
  final VoidCallback onPick;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return InkWell(
      onTap: onPick,
      borderRadius: BorderRadius.circular(72),
      child: Ink(
        width: 192,
        height: 192,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: colors.backgroundBase.quaternary,
          image: picture != null
              ? DecorationImage(image: MemoryImage(picture!), fit: BoxFit.cover)
              : null,
        ),
        child: picture == null
            ? const Center(
                child: IconTheme(
                  data: IconThemeData(),
                  child: AppIcon.imagePlus(size: 24),
                ),
              )
            : null,
      ),
    );
  }
}

class _GroupCreationAppBarTitle extends StatelessWidget {
  const _GroupCreationAppBarTitle({
    required this.title,
    required this.leading,
    required this.trailing,
  });

  final String title;
  final Widget leading;
  final Widget trailing;

  @override
  Widget build(BuildContext context) {
    return _CenteredAppBarTitle(
      title: title,
      leading: leading,
      trailing: trailing,
    );
  }
}

class _CenteredAppBarTitle extends StatelessWidget {
  const _CenteredAppBarTitle({
    required this.title,
    required this.leading,
    required this.trailing,
  });

  final String title;
  final Widget leading;
  final Widget trailing;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Flexible(
          flex: 1,
          child: Align(alignment: Alignment.centerLeft, child: leading),
        ),
        Expanded(
          child: Center(
            child: Text(
              title,
              style: Theme.of(context).appBarTheme.titleTextStyle,
            ),
          ),
        ),
        Flexible(
          flex: 1,
          child: Align(alignment: Alignment.centerRight, child: trailing),
        ),
      ],
    );
  }
}

class _SelectedParticipant extends StatelessWidget {
  const _SelectedParticipant({required this.profile, required this.onRemove});

  final UiUserProfile profile;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return SizedBox(
      width: 72,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Stack(
            clipBehavior: Clip.none,
            children: [
              UserAvatar(userId: profile.userId, size: 48),
              Positioned(
                top: -2,
                right: -2,
                child: GestureDetector(
                  onTap: onRemove,
                  child: Container(
                    width: 16,
                    height: 16,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: colors.text.primary,
                      border: Border.all(
                        color: colors.backgroundBase.primary,
                        width: 1,
                      ),
                    ),
                    child: Center(
                      child: AppIcon.x(
                        size: 10,
                        color: colors.backgroundBase.primary,
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: Spacings.xxs),
          Text(
            profile.displayName,
            textAlign: TextAlign.center,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(
              context,
            ).textTheme.labelSmall?.copyWith(height: 1.2),
          ),
        ],
      ),
    );
  }
}

class _CircularBackButton extends StatelessWidget {
  const _CircularBackButton({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
      child: InkWell(
        onTap: onPressed,
        borderRadius: BorderRadius.circular(18),
        child: Ink(
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: colors.backgroundBase.secondary,
          ),
          child: const Center(child: AppIcon.arrowLeft(size: 16)),
        ),
      ),
    );
  }
}
