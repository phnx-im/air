// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/share/staged_share.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';

final _log = Logger('ShareCubit');

/// Holds the share waiting to be staged in a chat's composer (Android share
/// handoff). Runs in the main app, unlike [IOSShareCubit], which backs the
/// share UI in the iOS share extension.
///
/// Single slot: a new share replaces an unconsumed one.
class AndroidShareCubit extends Cubit<StagedShare?> {
  AndroidShareCubit() : super(null);

  void stage(StagedShare share) {
    final previous = state;
    emit(share);
    if (previous != null) {
      unawaited(_deleteFiles(previous.attachments));
    }
  }

  /// Takes the pending share destined for [chatId], if any.
  StagedShare? take(ChatId chatId, {bool acceptUnaddressed = false}) {
    final share = state;
    if (share == null) {
      return null;
    }
    final destination = share.chatId;
    if (destination != null ? destination != chatId : !acceptUnaddressed) {
      return null;
    }
    emit(null);
    return share;
  }

  /// Lets the user pick the destination from the chat list instead.
  void unaddress() {
    final share = state;
    if (share == null || share.chatId == null) {
      return;
    }
    emit(
      StagedShare(
        attachments: share.attachments,
        text: share.text,
        droppedAttachments: share.droppedAttachments,
      ),
    );
  }

  /// Drops the pending share and deletes its extracted files.
  void dropPendingShare() {
    final share = state;
    emit(null);
    if (share != null) {
      unawaited(_deleteFiles(share.attachments));
    }
  }

  Future<void> _deleteFiles(List<UiSharedAttachment> attachments) async {
    for (final attachment in attachments) {
      try {
        await File(attachment.path).delete();
      } catch (e) {
        _log.warning("Failed to delete dropped share file: $e");
      }
    }
  }
}

class IOSShareCubit implements StateStreamableSource<ShareState> {
  IOSShareCubit({required String dbPath})
    : _impl = ShareCubitBase(dbPath: dbPath);

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
