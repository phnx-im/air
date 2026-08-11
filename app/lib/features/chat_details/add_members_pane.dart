// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_guard.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:logging/logging.dart';

import 'package:air/features/chat_details/add_members_cubit.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/member_search_field.dart';
import 'package:air/features/chat_details/member_selection_list.dart';

final _log = Logger('AddMembersPane');

/// Contacts to invite into a group.
class AddMembersPane extends StatelessWidget {
  const AddMembersPane({super.key, required this.chatId});

  final ChatId chatId;

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) =>
          AddMembersCubit()
            ..loadContacts(context.read<UserCubit>().addableContacts(chatId)),
      child: AddMembersView(chatId: chatId),
    );
  }
}

class AddMembersView extends StatefulWidget {
  const AddMembersView({super.key, required this.chatId});

  final ChatId chatId;

  @override
  State<AddMembersView> createState() => _AddMembersViewState();
}

class _AddMembersViewState extends State<AddMembersView> {
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
    final isApq = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isApq ?? false,
    );
    final loc = AppLocalizations.of(context);

    return ModalPane(
      title: loc.addMembersScreen_addMembers,
      trailing: Button(
        size: ButtonSize.small,
        state: selectedContacts.isEmpty
            ? ButtonState.disabled
            : ButtonState.active,
        label: loc.addMembersScreen_done,
        onPressed: () => _addSelectedContacts(context, selectedContacts),
      ),
      // The selection list below the search field scrolls on its own.
      scrollable: false,
      child: ModalDismissGuard(
        hasUnsavedInput: () =>
            context.read<AddMembersCubit>().state.selectedContacts.isNotEmpty,
        child: Column(
          children: [
            MemberSearchField(
              controller: _searchController,
              hintText: loc.groupMembersScreen_searchHint,
              onChanged: (value) => setState(() => _query = value),
            ),
            Expanded(
              child: AppScrollbar(
                child: ScrollConfiguration(
                  behavior: ScrollConfiguration.of(
                    context,
                  ).copyWith(scrollbars: false),
                  child: MemberSelectionList(
                    contacts: contacts,
                    selectedContacts: selectedContacts,
                    query: _query,
                    isApq: isApq,
                    onToggle: (contact) => context
                        .read<AddMembersCubit>()
                        .toggleContact(contact.userId),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _addSelectedContacts(
    BuildContext context,
    Set<UiUserId> selectedContacts,
  ) async {
    final navigationCubit = context.read<NavigationCubit>();
    final userCubit = context.read<UserCubit>();
    final error = await userCubit.addUserToChat(
      widget.chatId,
      selectedContacts.toList(),
    );
    switch (error) {
      // No error
      case null:
        navigationCubit.pop();
        break;
      case InviteUsersError_IncompatibleClient(:final reason):
        _log.severe('Failed to add members: incompatible client', reason);
        showErrorBannerStandalone(
          (loc) => loc.addMembersScreen_error_incompatibleClient,
        );
    }
  }
}
