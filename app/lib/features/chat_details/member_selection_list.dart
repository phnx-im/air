// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/checkbox/checkbox.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

import 'package:air/features/chat_details/member_list_item.dart';

/// A row of the list: either a contact or the header of the section holding
/// the contacts that can't join.
sealed class _Row {
  const _Row();
}

class _ContactRow extends _Row {
  const _ContactRow(this.contact, {required this.isSupported});

  final UiContact contact;
  final bool isSupported;
}

class _UnsupportedHeaderRow extends _Row {
  const _UnsupportedHeaderRow();
}

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

    final rows = useMemoized(() {
      final filteredContacts = normalizedQuery.isEmpty
          ? contacts
          : contacts.where((contact) {
              final name = profiles[contact.userId]!.displayName.toLowerCase();
              return name.contains(normalizedQuery);
            });
      final sortedContacts = filteredContacts.sortedBy(
        (contact) => profiles[contact.userId]!.displayName.toLowerCase(),
      );

      // Contacts that can't join go into their own section at the bottom, so
      // the selectable ones stay together at the top.
      final selectable = <_Row>[];
      final unsupported = <_Row>[];
      for (final contact in sortedContacts) {
        if (contact.isSupported(isApq: isApq)) {
          selectable.add(_ContactRow(contact, isSupported: true));
        } else {
          unsupported.add(_ContactRow(contact, isSupported: false));
        }
      }
      return [
        ...selectable,
        if (unsupported.isNotEmpty) const _UnsupportedHeaderRow(),
        ...unsupported,
      ];
    }, [contacts, profiles, normalizedQuery, isApq]);

    return ListView.separated(
      // The modal surface runs to the bottom of the screen, so the last row
      // clears the home indicator on its own rather than through a SafeArea.
      padding: EdgeInsets.fromLTRB(
        S.s16,
        S.s12,
        S.s16,
        S.s12 + MediaQuery.viewPaddingOf(context).bottom,
      ),
      itemCount: rows.length,
      separatorBuilder: (context, index) {
        // The section header carries its own spacing, so only contacts are
        // divided from each other.
        if (rows[index] is! _ContactRow || rows[index + 1] is! _ContactRow) {
          return const SizedBox.shrink();
        }
        return Divider(
          height: 1,
          thickness: StrokeWidth.px0_5,
          color: SemanticPalette.of(context).separator.secondary,
        );
      },
      itemBuilder: (context, index) {
        switch (rows[index]) {
          case _UnsupportedHeaderRow():
            return const _UnsupportedSectionHeader();
          case _ContactRow(:final contact, :final isSupported):
            final profile = profiles[contact.userId]!;
            final isSelected = selectedContacts.contains(contact.userId);

            return Opacity(
              opacity: isSupported ? 1.0 : Alpha.a50,
              child: MemberListItem(
                profile: profile,
                onTap: isSupported
                    ? () => onToggle(contact)
                    : () => showSnackBarStandalone(
                        (loc) => SnackBar(
                          content: Text(
                            loc.memberSelectionList_client_not_supported,
                          ),
                        ),
                      ),
                trailing: isSupported
                    ? AppCheckbox(
                        value: isSelected,
                        onChanged: (_) => onToggle(contact),
                      )
                    : null,
              ),
            );
        }
      },
    );
  }
}

/// Heads the section of contacts that can't join, and says why.
class _UnsupportedSectionHeader extends StatelessWidget {
  const _UnsupportedSectionHeader();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);

    return Padding(
      padding: const EdgeInsets.only(top: S.s24, bottom: S.s8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            loc.memberSelectionList_cantBeAddedTitle,
            style: typeScale.body.regular.style(
              color: palette.text.primary,
              weight: Weight.emphasized,
            ),
          ),
          const SizedBox(height: S.s4),
          Text(
            loc.memberSelectionList_cantBeAddedDescription,
            style: typeScale.body.s.style(color: palette.text.tertiary),
          ),
        ],
      ),
    );
  }
}
