// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class LinkedDevicesCubit implements StateStreamableSource<LinkedDevicesState> {
  LinkedDevicesCubit({required UserCubit userCubit})
    : _impl = LinkedDevicesCubitBase(userCubit: userCubit.impl);

  final LinkedDevicesCubitBase _impl;

  LinkedDevicesCubitBase get impl => _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  LinkedDevicesState get state => _impl.state;

  @override
  Stream<LinkedDevicesState> get stream => _impl.stream();

  Future<void> renameDevice({
    required String clientId,
    required String name,
  }) async => _impl.renameDevice(clientId: clientId, name: name);
}
