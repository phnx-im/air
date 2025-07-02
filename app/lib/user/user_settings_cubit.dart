// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:prototype/core/core.dart';

import 'user_cubit.dart';

class UserSettingsCubit implements StateStreamableSource<UserSettings> {
  UserSettingsCubit() : _impl = UserSettingsCubitBase();

  final UserSettingsCubitBase _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  UserSettings get state => _impl.state;

  @override
  Stream<UserSettings> get stream => _impl.stream();

  // Cubit methods

  Future<void> reset() => _impl.reset();

  Future<void> loadState({required User user}) => _impl.loadState(user: user);

  Future<void> setInterfaceScale({
    required UserCubit userCubit,
    required double value,
  }) => _impl.setInterfaceScale(userCubit: userCubit.impl, value: value);

  Future<void> setSidebarWidth({
    required UserCubit userCubit,
    required double value,
  }) => _impl.setSidebarWidth(userCubit: userCubit.impl, value: value);
}
