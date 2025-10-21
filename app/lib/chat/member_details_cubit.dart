// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';

class MemberDetailsCubit extends StateStreamableSource<MemberDetailsState> {
  MemberDetailsCubit({required UserCubit userCubit, required ChatId chatId})
    : _impl = MemberDetailsCubitBase(userCubit: userCubit.impl, chatId: chatId);

  final MemberDetailsCubitBase _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  MemberDetailsState get state => _impl.state;

  @override
  Stream<MemberDetailsState> get stream => _impl.stream();
}
