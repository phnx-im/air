// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/core/core.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';

import 'add_members_cubit.dart';

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

class AddMembersScreenView extends StatelessWidget {
  const AddMembersScreenView({super.key});

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
        title: Text(loc.addMembersScreen_title),
      ),
      body: Padding(
        padding: const EdgeInsets.all(32.0),
        child: Center(
          child: Container(
            constraints: const BoxConstraints(minWidth: 100, maxWidth: 600),
            child: Column(
              children: [
                Expanded(
                  child: ListView.builder(
                    itemCount: contacts.length,
                    itemBuilder: (context, index) {
                      final contact = contacts[index];
                      return _MemberTile(
                        contact: contact,
                        selectedContacts: selectedContacts,
                      );
                    },
                  ),
                ),
                OutlinedButton(
                  onPressed:
                      selectedContacts.isNotEmpty
                          ? () async {
                            _addSelectedContacts(context, selectedContacts);
                          }
                          : null,
                  child: Text(loc.addMembersScreen_addMembers),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  void _addSelectedContacts(
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

class _MemberTile extends StatelessWidget {
  const _MemberTile({required this.contact, required this.selectedContacts});

  final UiContact contact;
  final Set<UiUserId> selectedContacts;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: contact.userId),
    );

    return ListTile(
      leading: UserAvatar(
        displayName: profile.displayName,
        image: profile.profilePicture,
      ),
      title: Text(
        profile.displayName,
        style: Theme.of(context).textTheme.bodyMedium,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Checkbox(
        value: selectedContacts.contains(contact.userId),
        checkColor: CustomColorScheme.of(context).text.secondary,
        fillColor: WidgetStateProperty.all(
          CustomColorScheme.of(context).backgroundBase.secondary,
        ),
        focusColor: Colors.transparent,
        hoverColor: Colors.transparent,
        overlayColor: WidgetStateProperty.all(Colors.transparent),
        side: BorderSide.none,
        shape: const CircleBorder(),
        onChanged:
            (bool? value) =>
                context.read<AddMembersCubit>().toggleContact(contact),
      ),
      onTap: () => context.read<AddMembersCubit>().toggleContact(contact),
    );
  }
}
