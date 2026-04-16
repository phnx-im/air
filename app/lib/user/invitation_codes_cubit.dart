// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/user/user_cubit.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class InvitationCodesCubit
    implements StateStreamableSource<InvitationCodesState> {
  InvitationCodesCubit({required UserCubit userCubit})
    : _impl = InvitationCodesCubitBase(userCubit: userCubit.impl);

  final InvitationCodesCubitBase _impl;

  InvitationCodesCubitBase get impl => _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  InvitationCodesState get state => _impl.state;

  @override
  Stream<InvitationCodesState> get stream => _impl.stream();

  // Cubit methods

  Future<RequestInvitationCodeError?> requestInvitationCode({
    required TokenId tokenId,
  }) async => _impl.requestInvitationCode(tokenId: tokenId);

  Future<void> markInvitationCodeAsCopied({required String copiedCode}) async =>
      _impl.markInvitationCodeAsCopied(copiedCode: copiedCode);
}
