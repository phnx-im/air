// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';

class MessageCubit extends StateStreamableSource<MessageState> {
  MessageCubit({
    required UserCubit userCubit,
    required MessageState initialState,
  }) : _impl = MessageCubitBase(
         userCubit: userCubit.impl,
         initialState: initialState,
       );

  final MessageCubitBase _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  MessageState get state => _impl.state;

  @override
  Stream<MessageState> get stream => _impl.stream();
}
