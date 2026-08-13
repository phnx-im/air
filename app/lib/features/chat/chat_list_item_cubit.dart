// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart' hide ChatsRepository;
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:freezed_annotation/freezed_annotation.dart';
import 'chats_repository.dart';

part 'chat_list_item_cubit.freezed.dart';

/// The state of one chat list row.
@freezed
sealed class ChatListItemState with _$ChatListItemState {
  const factory ChatListItemState({required UiChatDetails chat}) =
      _ChatListItemState;
}

/// Cubit for a single chat list item.
///
/// Note: This technically duplicates the chat details cubit, but we don't want
/// to replace the latter everywhere at once, so for now we live the the
/// duplication.
class ChatListItemCubit extends Cubit<ChatListItemState> {
  ChatListItemCubit({required ChatsRepository repository, required this.chatId})
    : _repository = repository,
      super(ChatListItemState(chat: repository.getChat(chatId))) {
    _sub = _repository.watchChat(chatId).listen((chat) {
      // Null mean the chat is gone. The corresponding row will be disposed, so
      // we don't update the state here.
      if (chat != null) emit(ChatListItemState(chat: chat));
    });
  }

  final ChatsRepository _repository;
  final ChatId chatId;
  late final StreamSubscription<UiChatDetails?> _sub;

  @override
  Future<void> close() {
    _sub.cancel();
    return super.close();
  }

  // Commands: the write lands in the database, the data source emits a delta,
  // the new state arrives through the subscription => no optimistic local
  // mutation => there is only a single path into the state.

  Future<void> mute({required UiChatMuted until}) =>
      _repository.mute(chatId, until: until);

  Future<void> unmute() => _repository.unmute(chatId);
}
