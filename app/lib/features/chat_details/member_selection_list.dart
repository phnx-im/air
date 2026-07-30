// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/dimensions.dart';
import 'package:air/ds/foundations/semantic_colors.dart';
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
      padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
      itemCount: sortedContacts.length,
      separatorBuilder: (context, index) => Divider(
        height: 1,
        thickness: Strokes.px1,
        color: SemanticColors.of(context).backgroundBase.primary,
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
          opacity: hasSupportedClient ? 1.0 : Opacities.alpha50,
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
                ? Checkbox(
                    value: isSelected,
                    checkColor: SemanticColors.of(context).text.secondary,
                    fillColor: WidgetStateProperty.all(
                      SemanticColors.of(context).fill.tertiary,
                    ),
                    focusColor: Colors.transparent,
                    hoverColor: Colors.transparent,
                    overlayColor: WidgetStateProperty.all(Colors.transparent),
                    side: BorderSide.none,
                    shape: const RoundedRectangleBorder(
                      borderRadius: BorderRadius.all(
                        Radius.circular(Radii.px4),
                      ),
                    ),
                    onChanged: (_) => onToggle(contact),
                  )
                : null,
          ),
        );
      },
    );
  }
}
