// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/toggle/toggle.dart';
import 'package:air/ds/components/toggle/toggle_tokens.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/chat_details/member_selection_list.dart';
import 'package:air/features/chat_details/member_search_field.dart';
import 'package:air/core/core.dart' hide ChatsRepository;
import 'package:air/l10n/app_localizations.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/field/field_chrome.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/features/user/avatar.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';

import 'package:air/features/chat_details/add_members_cubit.dart';

final _log = Logger('CreateGroupScreen');

class CreateGroupScreen extends StatelessWidget {
  const CreateGroupScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) {
        final userCubit = context.read<UserCubit>();
        final contactsFuture = userCubit.contacts;
        return AddMembersCubit()..loadContacts(contactsFuture);
      },
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

    final (contacts, selectedContacts, isApq) = context.select(
      (AddMembersCubit cubit) => (
        cubit.state.contacts,
        cubit.state.selectedContacts,
        cubit.state.isApq,
      ),
    );
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        automaticallyImplyLeading: false,
        titleSpacing: 0,
        title: _CenteredAppBarTitle(
          title: loc.groupCreationScreen_title,
          leading: _CircularBackButton(
            onPressed: () => context.read<NavigationCubit>().pop(),
          ),
          trailing: Padding(
            padding: const EdgeInsets.symmetric(horizontal: S.s16),
            child: Button(
              size: ButtonSize.small,
              type: ButtonType.secondary,
              label: loc.groupCreationScreen_next,
              onPressed: () {
                FocusScope.of(context).unfocus();
                onNext();
              },
            ),
          ),
        ),
      ),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: Container(
            constraints: DeviceType.isDesktop
                ? const BoxConstraints(maxWidth: Measure.m800)
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
                    isApq: isApq,
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
    final (selectedIds, isApq) = context.select(
      (AddMembersCubit cubit) =>
          (cubit.state.selectedContacts, cubit.state.isApq),
    );

    final selectedProfiles = context.select(
      (UsersCubit cubit) => {
        for (final userId in selectedIds)
          userId: cubit.state.profile(userId: userId),
      },
    );

    final selectedFeatures = context.select(
      (AddMembersCubit cubit) => {
        for (final contact in cubit.state.contacts)
          if (selectedIds.contains(contact.userId))
            contact.userId: contact.supportedFeatures,
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
    final showHiddenSettings = useState(false);

    final isGroupNameValid = groupName.value.trim().isNotEmpty;
    final showHelperText = nameFocusNode.hasFocus && !isGroupNameValid;
    final createState = switch ((isCreating.value, isGroupNameValid)) {
      (true, _) => ButtonState.pending,
      (false, true) => ButtonState.active,
      (false, false) => ButtonState.disabled,
    };

    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        automaticallyImplyLeading: false,
        titleSpacing: 0,
        title: _CenteredAppBarTitle(
          title: loc.groupCreationDetails_title,
          leading: _CircularBackButton(
            onPressed: () => _handleBack(context, isCreating.value),
          ),
          trailing: Padding(
            padding: const EdgeInsets.symmetric(horizontal: S.s16),
            child: Button(
              size: ButtonSize.small,
              type: ButtonType.secondary,
              state: createState,
              label: loc.groupCreationDetails_create,
              onPressed: () => _createGroupChat(
                context,
                groupName.value.trim(),
                isCreating,
                picture.value,
                isApq,
              ),
            ),
          ),
          onLongPress: () {
            showHiddenSettings.value = !showHiddenSettings.value;
          },
        ),
      ),
      body: SafeArea(
        child: GestureDetector(
          onTap: () => FocusScope.of(context).unfocus(),
          behavior: HitTestBehavior.translucent,
          child: SingleChildScrollView(
            padding: const EdgeInsets.symmetric(
              horizontal: S.s24,
              vertical: S.s24,
            ),
            child: Align(
              alignment: Alignment.topCenter,
              child: Container(
                constraints: DeviceType.isDesktop
                    ? const BoxConstraints(maxWidth: Measure.m800)
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
                    const SizedBox(height: S.s32),
                    SizedBox(
                      width: double.infinity,
                      child: TextField(
                        onChanged: (value) => groupName.value = value,
                        focusNode: nameFocusNode,
                        textInputAction: TextInputAction.next,
                        textAlign: TextAlign.center,
                        style: Theme.of(context).textTheme.displayLarge
                            ?.copyWith(fontWeight: FontWeight.bold),
                        decoration: FieldChrome.plain(
                          hintText: nameFocusNode.hasFocus
                              ? loc.groupCreationDetails_groupNameHintFocused
                              : loc.groupCreationDetails_groupNameHint,
                          hintStyle: Theme.of(context).textTheme.displayLarge
                              ?.copyWith(
                                color: palette.text.quaternary,
                                fontWeight: FontWeight.bold,
                              ),
                        ),
                      ),
                    ),
                    if (showHelperText) ...[
                      const SizedBox(height: S.s8),
                      Center(
                        child: Text(
                          loc.groupCreationDetails_groupNameHelper,
                          textAlign: TextAlign.center,
                          style: Theme.of(context).textTheme.bodySmall
                              ?.copyWith(color: palette.text.tertiary),
                        ),
                      ),
                    ],
                    if (showHiddenSettings.value) ...[
                      const SizedBox(height: S.s32),
                      _SwitchField(
                        onChanged: (value) {
                          context.read<AddMembersCubit>().enableApq(value);
                        },
                        value: isApq,
                        label: "Post-Quantum Encryption",
                      ),
                    ],
                    const SizedBox(height: S.s32),
                    if (selectedIds.isNotEmpty)
                      Wrap(
                        alignment: WrapAlignment.start,
                        spacing: S.s16,
                        runSpacing: S.s16,
                        children: sortedSelectedIds.map((userId) {
                          final profile = selectedProfiles[userId];
                          if (profile == null) {
                            return const SizedBox.shrink();
                          }
                          final features = selectedFeatures[userId];
                          final isSupported =
                              features?.isSupported(isApq: isApq) ?? false;
                          return Opacity(
                            opacity: isSupported ? 1.0 : Alpha.a50,
                            child: _SelectedParticipant(
                              profile: profile,
                              onRemove: () => _removeContact(context, userId),
                            ),
                          );
                        }).toList(),
                      )
                    else
                      Center(
                        child: Text(
                          loc.groupCreationDetails_emptySelection,
                          textAlign: TextAlign.center,
                          style: Theme.of(context).textTheme.bodyMedium
                              ?.copyWith(color: palette.text.tertiary),
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
    // Reduce image quality to re-encode the image.
    final XFile? image = await picker.pickImage(
      source: ImageSource.gallery,
      imageQuality: 99,
    );
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
    bool isApq,
  ) async {
    if (groupName.isEmpty || isCreating.value) return;

    final navigationCubit = context.read<NavigationCubit>();
    final userCubit = context.read<UserCubit>();
    final addMembersCubit = context.read<AddMembersCubit>();
    final selectedContacts = addMembersCubit.state.selectedContacts;
    final supportedSelectedContacts = addMembersCubit.state.contacts
        .where((contact) {
          if (!selectedContacts.contains(contact.userId)) return false;
          return contact.isSupported(isApq: isApq);
        })
        .map((contact) => contact.userId)
        .toList();

    isCreating.value = true;

    final chatsRepository = context.read<ChatsRepository>();

    try {
      final chatId = await chatsRepository.createGroupChat(
        groupName: groupName,
        picture: picture,
        isApq: isApq,
      );
      final error = await userCubit.addUserToChat(
        chatId,
        supportedSelectedContacts,
      );
      switch (error) {
        // No error
        case null:
          if (!context.mounted) return;
          navigationCubit.pop();
          await navigationCubit.openChat(chatId);
          break;
        case InviteUsersError_IncompatibleClient(:final reason):
          _log.severe(
            'Failed to create group "$groupName" due to incompatible client: $reason',
            reason,
          );
          showErrorBannerStandalone(
            (loc) => loc.newChatDialog_error_incompatibleClient(groupName),
          );
          break;
      }
    } catch (error, stackTrace) {
      _log.severe(
        'Failed to create group "$groupName": $error',
        error,
        stackTrace,
      );
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
    final palette = SemanticPalette.of(context);
    return InkWell(
      onTap: onPick,
      borderRadius: BorderRadius.circular(CornerRadius.full),
      child: Ink(
        width: 192,
        height: 192,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: palette.backgroundBase.quaternary,
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

class _CenteredAppBarTitle extends StatelessWidget {
  const _CenteredAppBarTitle({
    required this.title,
    required this.leading,
    required this.trailing,
    this.onLongPress,
  });

  final String title;
  final Widget leading;
  final Widget trailing;
  final VoidCallback? onLongPress;

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
            child: InkWell(
              onLongPress: onLongPress,
              child: Text(
                title,
                style: Theme.of(context).appBarTheme.titleTextStyle,
              ),
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
    final palette = SemanticPalette.of(context);
    return SizedBox(
      width: 72,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Stack(
            clipBehavior: Clip.none,
            children: [
              UserAvatar(profile: profile, size: 48),
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
                      color: palette.text.primary,
                      border: Border.all(
                        color: palette.backgroundBase.primary,
                        width: StrokeWidth.px1,
                      ),
                    ),
                    child: Center(
                      child: AppIcon.x(
                        size: 10,
                        color: palette.backgroundBase.primary,
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: S.s8),
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
    final palette = SemanticPalette.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s16),
      child: InkWell(
        onTap: onPressed,
        borderRadius: BorderRadius.circular(CornerRadius.full),
        child: Ink(
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: palette.backgroundBase.secondary,
          ),
          child: const Center(child: AppIcon.arrowLeft(size: 16)),
        ),
      ),
    );
  }
}

/// A switch field that reflects [value] and reports toggles via [onChanged].
class _SwitchField extends StatelessWidget {
  const _SwitchField({
    required this.onChanged,
    required this.value,
    required this.label,
  });

  final ValueChanged<bool> onChanged;
  final bool value;
  final String label;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return InkWell(
      onTap: () => onChanged(!value),
      child: Container(
        decoration: BoxDecoration(
          color: palette.backgroundBase.secondary,
          borderRadius: BorderRadius.circular(CornerRadius.px16),
        ),
        padding: const EdgeInsets.symmetric(horizontal: S.s12),
        height: 42,
        child: Row(
          children: [
            Text(
              label,
              style: typeScale.body.regular.style(color: palette.text.primary),
            ),
            const Spacer(),
            Toggle(
              tokens: ToggleTokens.current,
              value: value,
              onChanged: onChanged,
            ),
          ],
        ),
      ),
    );
  }
}
