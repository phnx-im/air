// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:collection';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:air/core/core.dart';

part 'add_members_cubit.freezed.dart';

@freezed
sealed class AddMembersState with _$AddMembersState {
  const factory AddMembersState({
    required List<UiContact> contacts,
    required Set<UiUserId> selectedContacts,
  }) = _AddMembersState;
}

class AddMembersCubit extends Cubit<AddMembersState> {
  AddMembersCubit()
    : super(const AddMembersState(contacts: [], selectedContacts: {}));

  void loadContacts(Future<List<UiContact>> futureContacts) async {
    final contacts = await futureContacts;
    emit(state.copyWith(contacts: contacts));
  }

  void toggleContact(UiContact contact) {
    final selectedContacts = HashSet<UiUserId>.from(state.selectedContacts);
    if (selectedContacts.contains(contact.userId)) {
      selectedContacts.remove(contact.userId);
    } else {
      selectedContacts.add(contact.userId);
    }
    emit(state.copyWith(selectedContacts: selectedContacts));
  }
}
