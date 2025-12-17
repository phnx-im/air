// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

import 'member_list_item.dart';

class MemberSelectionList extends HookWidget {
  const MemberSelectionList({
    super.key,
    required this.contacts,
    required this.selectedContacts,
    required this.query,
    required this.onToggle,
  });

  final List<UiContact> contacts;
  final Set<UiUserId> selectedContacts;
  final String query;
  final ValueChanged<UiContact> onToggle;

  @override
  Widget build(BuildContext context) {
    final profiles = context.select(
      (UsersCubit cubit) => {
        for (final contact in contacts)
          contact.userId: cubit.state.profile(userId: contact.userId),
      },
    );

    final normalizedQuery = query.trim().toLowerCase();

    final sortedContacts = useMemoized(() {
      final filteredContacts = normalizedQuery.isEmpty
          ? contacts
          : contacts.where((contact) {
              final name = profiles[contact.userId]!.displayName.toLowerCase();
              return name.contains(normalizedQuery);
            });
      return filteredContacts.sortedBy(
        (contact) => profiles[contact.userId]!.displayName.toLowerCase(),
      );
    }, [contacts, profiles, normalizedQuery]);

    return ListView.separated(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.m,
        vertical: Spacings.xs,
      ),
      itemCount: sortedContacts.length,
      separatorBuilder: (context, index) => Divider(
        height: 1,
        thickness: 1,
        color: CustomColorScheme.of(context).backgroundBase.primary,
      ),
      itemBuilder: (context, index) {
        final contact = sortedContacts[index];
        final profile = profiles[contact.userId]!;
        final isSelected = selectedContacts.contains(contact.userId);

        return MemberListItem(
          profile: profile,
          onTap: () => onToggle(contact),
          trailing: Checkbox(
            value: isSelected,
            checkColor: CustomColorScheme.of(context).text.secondary,
            fillColor: WidgetStateProperty.all(
              CustomColorScheme.of(context).fill.tertiary,
            ),
            focusColor: Colors.transparent,
            hoverColor: Colors.transparent,
            overlayColor: WidgetStateProperty.all(Colors.transparent),
            side: BorderSide.none,
            shape: const RoundedRectangleBorder(
              borderRadius: BorderRadius.all(Radius.circular(4)),
            ),
            onChanged: (_) => onToggle(contact),
          ),
        );
      },
    );
  }
}
