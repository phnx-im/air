// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';

class MessageListCubit extends StateStreamableSource<MessageListState> {
  MessageListCubit({required UserCubit userCubit, required ChatId chatId})
    : _impl = MessageListCubitBase(userCubit: userCubit.impl, chatId: chatId);

  final MessageListCubitBase _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  MessageListState get state => _impl.state;

  @override
  Stream<MessageListState> get stream => _impl.stream();
}
