// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

part 'chat_list_cubit.freezed.dart';

/// Represents the state of the list of chat.
@freezed
sealed class ChatListState with _$ChatListState {
  const factory ChatListState({required List<ChatId> chatIds}) = _ChatListState;
}

class ChatListCubit extends Cubit<ChatListState> {
  ChatListCubit({required this._chatRepository})
    : super(ChatListState(chatIds: _chatRepository.order)) {
    _order = _chatRepository.watchOrder().listen((order) {
      // Note: emit() deduplicates the same state
      emit(ChatListState(chatIds: order));
    });
  }

  final ChatsRepository _chatRepository;
  late final StreamSubscription<List<ChatId>> _order;

  @override
  Future<void> close() async {
    await _order.cancel();
    return super.close();
  }
}
