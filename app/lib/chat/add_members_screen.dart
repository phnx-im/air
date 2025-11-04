// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/widgets/app_bar_button.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/core/core.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';

import 'add_members_cubit.dart';
import 'widgets/member_search_field.dart';
import 'widgets/member_selection_list.dart';

class AddMembersScreen extends StatelessWidget {
  const AddMembersScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) {
        final userCubit = context.read<UserCubit>();
        final navigationCubit = context.read<NavigationCubit>();
        final chatId = navigationCubit.state.chatId;
        final contactsFuture =
            chatId != null
                ? userCubit.addableContacts(chatId)
                : Future.value(<UiContact>[]);

        return AddMembersCubit()..loadContacts(contactsFuture);
      },
      child: const AddMembersScreenView(),
    );
  }
}

class AddMembersScreenView extends StatefulWidget {
  const AddMembersScreenView({super.key});

  @override
  State<AddMembersScreenView> createState() => _AddMembersScreenViewState();
}

class _AddMembersScreenViewState extends State<AddMembersScreenView> {
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
      (AddMembersCubit cubit) => (
        cubit.state.contacts,
        cubit.state.selectedContacts,
      ),
    );
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        leading: const AppBarBackButton(),
        title: Text(loc.addMembersScreen_addMembers),
        actions: [
          AppBarButton(
            onPressed:
                selectedContacts.isNotEmpty
                    ? () => _addSelectedContacts(context, selectedContacts)
                    : null,

            child: Text(loc.addMembersScreen_done),
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
                  controller: _searchController,
                  hintText: loc.groupMembersScreen_searchHint,
                  onChanged: (value) => setState(() => _query = value),
                ),
                Expanded(
                  child: MemberSelectionList(
                    contacts: contacts,
                    selectedContacts: selectedContacts,
                    query: _query,
                    onToggle:
                        (contact) => context
                            .read<AddMembersCubit>()
                            .toggleContact(contact),
                  ),
                ),
              ],
            ),
          ),
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
    final chatId = navigationCubit.state.chatId;
    final loc = AppLocalizations.of(context);
    if (chatId == null) {
      throw StateError(loc.addMembersScreen_error_noActiveChat);
    }
    for (final userId in selectedContacts) {
      await userCubit.addUserToChat(chatId, userId);
    }
    navigationCubit.pop();
  }
}
