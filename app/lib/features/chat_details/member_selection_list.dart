// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/checkbox/checkbox.dart';
import 'package:air/ds/components/checkbox/checkbox_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

import 'package:air/features/chat_details/member_list_item.dart';

class MemberSelectionList extends HookWidget {
  const MemberSelectionList({
    super.key,
    required this.contacts,
    required this.selectedContacts,
    required this.query,
    required this.onToggle,
    this.isApq = false,
  });

  final List<UiContact> contacts;
  final Set<UiUserId> selectedContacts;
  final String query;
  final ValueChanged<UiContact> onToggle;
  final bool isApq;

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
      // The modal surface runs to the bottom of the screen, so the last row
      // clears the home indicator on its own rather than through a SafeArea.
      padding: EdgeInsets.fromLTRB(
        S.s16,
        S.s12,
        S.s16,
        S.s12 + MediaQuery.viewPaddingOf(context).bottom,
      ),
      itemCount: sortedContacts.length,
      separatorBuilder: (context, index) => Divider(
        height: 1,
        thickness: StrokeWidth.px1,
        color: SemanticPalette.of(context).backgroundBase.primary,
      ),
      itemBuilder: (context, index) {
        final contact = sortedContacts[index];
        final profile = profiles[contact.userId]!;
        final isSelected = selectedContacts.contains(contact.userId);
        final features = contact.supportedFeatures;
        final hasEncryptedGroupProfiles =
            features?.encryptedGroupProfiles ?? false;
        final hasPqGroups = features?.pqGroups ?? false;
        final hasSupportedClient =
            hasEncryptedGroupProfiles && (!isApq || hasPqGroups);

        return Opacity(
          opacity: hasSupportedClient ? 1.0 : Alpha.a50,
          child: MemberListItem(
            profile: profile,
            onTap: hasSupportedClient
                ? () => onToggle(contact)
                : () => showSnackBarStandalone(
                    (loc) => SnackBar(
                      content: Text(
                        loc.memberSelectionList_client_not_supported,
                      ),
                    ),
                  ),
            trailing: hasSupportedClient
                ? AppCheckbox(
                    tokens: CheckboxTokens.standard,
                    value: isSelected,
                    onChanged: (_) => onToggle(contact),
                  )
                : null,
          ),
        );
      },
    );
  }
}
