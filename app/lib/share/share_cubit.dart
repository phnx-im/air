// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class ShareCubit implements StateStreamableSource<ShareState> {
  ShareCubit({required String dbPath}) : _impl = ShareCubitBase(dbPath: dbPath);

  final ShareCubitBase _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  ShareState get state => _impl.state;

  @override
  Stream<ShareState> get stream => _impl.stream();

  ChatId? chatIdForShareTarget(String identifier) =>
      _impl.chatIdForShareTarget(identifier);

  void resetSendStatus() => _impl.resetSendStatus();

  Future<void> send({
    required List<ChatId> chatIds,
    required List<UiSharedAttachment> attachments,
    String? message,
  }) =>
      _impl.send(chatIds: chatIds, attachments: attachments, message: message);
}
