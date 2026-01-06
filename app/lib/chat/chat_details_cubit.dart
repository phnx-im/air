// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';

class ChatDetailsCubit extends StateStreamableSource<ChatDetailsState> {
  ChatDetailsCubit({
    required UserCubit userCubit,
    required UserSettingsCubit userSettingsCubit,
    required ChatsRepository chatsRepository,
    required AttachmentsRepository attachmentsRepository,
    required ChatId chatId,

    /// Whether to load the chat members too.
    bool withMembers = true,
  }) : _impl = ChatDetailsCubitBase(
         userCubit: userCubit.impl,
         userSettingsCubit: userSettingsCubit.impl,
         chatId: chatId,
         chatsRepository: chatsRepository,
         attachmentsRepository: attachmentsRepository,
         withMembers: withMembers,
       );

  final ChatDetailsCubitBase _impl;

  @override
  FutureOr<void> close() {
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  ChatDetailsState get state => _impl.state;

  @override
  Stream<ChatDetailsState> get stream => _impl.stream();

  // Cubit methods

  Future<void> setChatPicture({required Uint8List? bytes}) =>
      _impl.setChatPicture(bytes: bytes);

  Future<void> setChatTitle({required String title}) =>
      _impl.setChatTitle(title: title);

  Future<void> sendMessage(String messageText) =>
      _impl.sendMessage(messageText: messageText);

  Future<void> deleteMessage() => _impl.deleteMessage();

  Future<void> uploadAttachment(String path) =>
      _impl.uploadAttachment(path: path);

  Future<void> retryUploadAttachment(AttachmentId attachmentId) =>
      _impl.retryUploadAttachment(attachmentId: attachmentId);

  Future<void> markAsRead({
    required MessageId untilMessageId,
    required DateTime untilTimestamp,
  }) => _impl.markAsRead(
    untilMessageId: untilMessageId,
    untilTimestamp: untilTimestamp,
  );

  Future<void> storeDraft({
    required String draftMessage,
    required bool isCommitted,
  }) => _impl.storeDraft(draftMessage: draftMessage, isCommitted: isCommitted);

  Future<void> resetDraft() => _impl.resetDraft();

  Future<void> editMessage({MessageId? messageId}) =>
      _impl.editMessage(messageId: messageId);

  Future<void> acceptContactRequest() => _impl.acceptContactRequest();
}
