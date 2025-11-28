// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class ChatListCubit implements StateStreamableSource<ChatListState> {
  ChatListCubit({required UserCubit userCubit})
    : _impl = ChatListCubitBase(userCubit: userCubit.impl);

  final ChatListCubitBase _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  ChatListState get state => _impl.state;

  @override
  Stream<ChatListState> get stream => _impl.stream();

  Future<ChatId?> createContactChat({required UiUserHandle handle}) async {
    final res = await _impl.createContactChat(handle: handle);

    switch (res) {
      case AddHandleContactResult_Ok(field0: final chatId):
        return chatId;
      case AddHandleContactResult_Err(field0: final error):
        switch (error) {
          case AddHandleContactError.handleNotFound:
            return null;
          case AddHandleContactError.duplicateRequest:
          case AddHandleContactError.ownHandle:
            throw Exception('Failed to create contact chat: $error');
        }
    }
  }

  Future<ChatId> createGroupChat({required String groupName}) =>
      _impl.createGroupChat(groupName: groupName);
}
