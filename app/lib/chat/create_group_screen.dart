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
import 'package:air/user/user.dart';
import 'package:air/widgets/avatar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:image_picker/image_picker.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;

import 'add_members_cubit.dart';

class CreateGroupScreen extends StatelessWidget {
  const CreateGroupScreen({super.key});

  @override
  Widget build(BuildContext context) {
    ChatListCubit? existingChatListCubit;
    try {
      existingChatListCubit = context.read<ChatListCubit>();
    } catch (_) {
      existingChatListCubit = null;
    }

    Widget flow;
    if (existingChatListCubit != null) {
      flow = _CreateGroupFlow(chatListCubit: existingChatListCubit);
    } else {
      flow = BlocProvider(
        create: (context) =>
            ChatListCubit(userCubit: context.read<UserCubit>()),
        child: Builder(
          builder: (innerContext) => _CreateGroupFlow(
            chatListCubit: innerContext.read<ChatListCubit>(),
          ),
        ),
      );
    }

    return BlocProvider(
      create: (context) {
        final userCubit = context.read<UserCubit>();
        final contactsFuture = userCubit.contacts;
        return AddMembersCubit()..loadContacts(contactsFuture);
      },
      child: flow,
    );
  }
}

class _CreateGroupFlow extends StatefulWidget {
  const _CreateGroupFlow({required this.chatListCubit});

  final ChatListCubit chatListCubit;

  @override
  State<_CreateGroupFlow> createState() => _CreateGroupFlowState();
}

class _CreateGroupFlowState extends State<_CreateGroupFlow> {
  bool _showDetails = false;

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: !_showDetails,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && _showDetails) {
          setState(() => _showDetails = false);
        }
      },
      child: IndexedStack(
        index: _showDetails ? 1 : 0,
        children: [
          _MemberSelectionStep(
            onNext: () => setState(() => _showDetails = true),
          ),
          _CreateGroupDetailsStep(
            chatListCubit: widget.chatListCubit,
            onBack: () => setState(() => _showDetails = false),
          ),
        ],
      ),
    );
  }
}

class _MemberSelectionStep extends StatefulWidget {
  const _MemberSelectionStep({required this.onNext});

  final VoidCallback onNext;

  @override
  State<_MemberSelectionStep> createState() => _MemberSelectionStepState();
}

class _MemberSelectionStepState extends State<_MemberSelectionStep> {
  final TextEditingController _searchController = TextEditingController();
  String _query = '';

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
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
              widget.onNext();
            },
            child: Text(
              selectedContacts.isEmpty
                  ? loc.groupCreationScreen_skip
                  : loc.groupCreationScreen_next,
            ),
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
                  controller: _searchController,
                  hintText: loc.groupMembersScreen_searchHint,
                  onChanged: (value) => setState(() => _query = value),
                ),
                Expanded(
                  child: MemberSelectionList(
                    contacts: contacts,
                    selectedContacts: selectedContacts,
                    query: _query,
                    onToggle: (contact) =>
                        context.read<AddMembersCubit>().toggleContact(contact),
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

class _CreateGroupDetailsStep extends StatefulWidget {
  const _CreateGroupDetailsStep({
    required this.onBack,
    required this.chatListCubit,
  });

  final VoidCallback onBack;
  final ChatListCubit chatListCubit;

  @override
  State<_CreateGroupDetailsStep> createState() =>
      _CreateGroupDetailsStepState();
}

class _CreateGroupDetailsStepState extends State<_CreateGroupDetailsStep> {
  final TextEditingController _nameController = TextEditingController();
  final FocusNode _nameFocusNode = FocusNode();
  Uint8List? _picture;
  bool _isCreating = false;

  @override
  void initState() {
    super.initState();
    _nameFocusNode.addListener(_handleFocusChange);
    _nameController.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _nameFocusNode.removeListener(_handleFocusChange);
    _nameFocusNode.dispose();
    _nameController.dispose();
    super.dispose();
  }

  void _handleFocusChange() => setState(() {});

  bool get _isGroupNameValid => _nameController.text.trim().isNotEmpty;

  bool get _showHelperText => _nameFocusNode.hasFocus && !_isGroupNameValid;

  @override
  Widget build(BuildContext context) {
    final addMembersState = context.watch<AddMembersCubit>().state;
    final usersState = context.watch<UsersCubit>().state;
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final selectedIds = addMembersState.selectedContacts.toList();
    final contactsById = {
      for (final contact in addMembersState.contacts) contact.userId: contact,
    };

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        automaticallyImplyLeading: false,
        titleSpacing: 0,
        title: _GroupCreationAppBarTitle(
          title: loc.groupCreationDetails_title,
          leading: _CircularBackButton(onPressed: _handleBack),
          trailing: AppBarButton(
            onPressed: _isGroupNameValid && !_isCreating
                ? _createGroupChat
                : null,
            child: _isCreating
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
                        picture: _picture,
                        onPick: _pickImage,
                      ),
                    ),
                    const SizedBox(height: Spacings.l),
                    SizedBox(
                      width: double.infinity,
                      child: TextField(
                        controller: _nameController,
                        focusNode: _nameFocusNode,
                        textInputAction: TextInputAction.next,
                        textAlign: TextAlign.center,
                        style: Theme.of(context).textTheme.displayLarge
                            ?.copyWith(fontWeight: FontWeight.bold),
                        decoration: InputDecoration(
                          hintText: _nameFocusNode.hasFocus
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
                    if (_showHelperText) ...[
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
                        children: selectedIds.map((userId) {
                          final profile = usersState.profile(userId: userId);
                          final contact = contactsById[userId];
                          if (contact == null) {
                            return const SizedBox.shrink();
                          }
                          return _SelectedParticipant(
                            profile: profile,
                            onRemove: () => _removeContact(contact),
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

  void _handleBack() {
    if (_isCreating) return;
    FocusScope.of(context).unfocus();
    widget.onBack();
  }

  Future<void> _pickImage() async {
    final picker = ImagePicker();
    final XFile? image = await picker.pickImage(source: ImageSource.gallery);
    if (image == null) {
      return;
    }
    final bytes = await image.readAsBytes();
    if (!mounted) return;
    setState(() => _picture = bytes);
  }

  void _removeContact(UiContact contact) {
    context.read<AddMembersCubit>().toggleContact(contact);
  }

  Future<void> _createGroupChat() async {
    final groupName = _nameController.text.trim();
    if (groupName.isEmpty) return;
    final navigationCubit = context.read<NavigationCubit>();
    final chatListCubit = widget.chatListCubit;
    final userCubit = context.read<UserCubit>();
    final addMembersCubit = context.read<AddMembersCubit>();
    final selectedContacts = addMembersCubit.state.selectedContacts;

    setState(() => _isCreating = true);

    try {
      final chatId = await chatListCubit.createGroupChat(
        groupName: groupName,
        picture: _picture,
      );
      for (final userId in selectedContacts) {
        await userCubit.addUserToChat(chatId, userId);
      }
      if (!mounted) return;
      navigationCubit.pop();
      await navigationCubit.openChat(chatId);
    } catch (error) {
      if (!mounted) return;
      setState(() => _isCreating = false);
      final loc = AppLocalizations.of(context);
      showErrorBanner(context, loc.newChatDialog_error(groupName));
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
            ? Center(
                child: IconTheme(
                  data: const IconThemeData(),
                  child: iconoir.MediaImagePlus(
                    width: 24,
                    color: colors.text.primary,
                  ),
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
                      child: iconoir.Xmark(
                        width: 10,
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
          child: Center(
            child: iconoir.ArrowLeft(width: 16, color: colors.text.primary),
          ),
        ),
      ),
    );
  }
}
