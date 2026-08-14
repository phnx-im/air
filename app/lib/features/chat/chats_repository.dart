// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:typed_data';

import 'package:air/core/core.dart';

abstract interface class ChatsRepository {
  bool get isLoaded;

  List<ChatId> get order;
  Stream<List<ChatId>> watchOrder();

  UiChatDetails? getChat(ChatId id);
  Stream<UiChatDetails?> watchChat(ChatId id);

  Future<void> mute(ChatId id, {required UiChatMuted until});
  Future<void> unmute(ChatId id);

  Future<AddUsernameContactError?> createContactChat({
    required UiUsername username,
    required UsernameHash hash,
  });

  Future<ChatId> createGroupChat({
    required String groupName,
    Uint8List? picture,
    required bool isApq,
  });

  Future<void> dispose();
}

class RustChatsRepository implements ChatsRepository {
  RustChatsRepository({required UserCubitBase userCubit})
    : _dataSource = ChatsDataSource(userCubit: userCubit) {
    _deltas = _dataSource.stream().listen(_apply);
  }

  // Data source and subscription

  final ChatsDataSource _dataSource;
  late final StreamSubscription<ChatsDelta> _deltas;

  // State

  final _chats = <ChatId, UiChatDetails>{};
  var _order = <ChatId>[];
  var _isLoaded = false;

  // Observable streams

  final _chatChanges = StreamController<Set<ChatId>>.broadcast();
  final _orderChanges = StreamController<List<ChatId>>.broadcast();

  // Public interface

  @override
  bool get isLoaded => _isLoaded;

  @override
  List<ChatId> get order => _order;

  @override
  Stream<List<ChatId>> watchOrder() => _orderChanges.stream;

  @override
  UiChatDetails? getChat(ChatId id) => _chats[id];

  @override
  Stream<UiChatDetails?> watchChat(ChatId id) => _chatChanges.stream
      .where((ids) => ids.contains(id))
      .map((_) => _chats[id]);

  @override
  Future<void> mute(ChatId id, {required UiChatMuted until}) =>
      _dataSource.mute(chatId: id, mutedUntil: until);

  @override
  Future<void> unmute(ChatId id) =>
      _dataSource.mute(chatId: id, mutedUntil: null);

  @override
  Future<AddUsernameContactError?> createContactChat({
    required UiUsername username,
    required UsernameHash hash,
  }) => _dataSource.createContactChat(username: username, hash: hash);

  @override
  Future<ChatId> createGroupChat({
    required String groupName,
    Uint8List? picture,
    required bool isApq,
  }) => _dataSource.createGroupChat(
    groupName: groupName,
    picture: picture,
    isApq: isApq,
  );

  @override
  Future<void> dispose() async {
    await _deltas.cancel();
    await _chatChanges.close();
    await _orderChanges.close();
    await _dataSource.close();
  }

  // Internals

  void _apply(ChatsDelta delta) {
    for (final chat in delta.upserted) {
      _chats[chat.id] = chat;
    }
    for (final chatId in delta.removed) {
      _chats.remove(chatId);
    }

    final order = delta.order;
    if (order != null) {
      _order = order;
      _isLoaded = true;
    }

    if (delta.upserted.isNotEmpty || delta.removed.isNotEmpty) {
      _chatChanges.add({...delta.upserted.map((c) => c.id), ...delta.removed});
    }

    if (order != null) {
      _orderChanges.add(order);
    }
  }
}
