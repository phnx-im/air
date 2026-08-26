// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:typed_data';

import 'package:air/core/core.dart';

abstract interface class ChatsRepository {
  bool get isLoaded;

  List<ChatId> get order;

  /// Must replay the current order on subscription.
  Stream<List<ChatId>> watchOrder();

  UiChatDetails? getChat(ChatId id);

  /// Ids of chats that changed or were removed.
  ///
  /// Does *not* replay.
  Stream<Set<ChatId>> watchChanges();

  /// Must replay the current chat details on subscription.
  Stream<UiChatDetails?> watchChat(ChatId id);
  Stream<List<UiUserId>?> watchMembers(ChatId id);

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
  final _members = <ChatId, List<UiUserId>>{};
  var _order = <ChatId>[];
  var _isLoaded = false;

  // Observable streams

  final _chatChanges = StreamController<Set<ChatId>>.broadcast();
  final _orderChanges = StreamController<List<ChatId>>.broadcast();
  final _memberChanges = StreamController<Set<ChatId>>.broadcast();

  // Public interface

  @override
  bool get isLoaded => _isLoaded;

  @override
  List<ChatId> get order => _order;

  @override
  Stream<List<ChatId>> watchOrder() => Stream.multi((controller) {
    controller.add(_order);
    final sub = _orderChanges.stream.listen(
      controller.add,
      onDone: controller.close,
    );
    controller.onCancel = sub.cancel;
  });

  @override
  UiChatDetails? getChat(ChatId id) => _chats[id];

  @override
  Stream<Set<ChatId>> watchChanges() => _chatChanges.stream;

  @override
  Stream<UiChatDetails?> watchChat(ChatId id) => Stream.multi((controller) {
    controller.add(_chats[id]);
    final sub = _chatChanges.stream
        .where((ids) => ids.contains(id))
        .listen((_) => controller.add(_chats[id]), onDone: controller.close);
    controller.onCancel = sub.cancel;
  });

  @override
  Stream<List<UiUserId>?> watchMembers(ChatId id) => Stream.multi((controller) {
    controller.add(_members[id]);
    final sub = _memberChanges.stream
        .where((ids) => ids.contains(id))
        .listen((_) => controller.add(_members[id]), onDone: controller.close);
    controller.onCancel = sub.cancel;
  });

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
    await _memberChanges.close();
    await _dataSource.close();
  }

  // Internals

  void _apply(ChatsDelta delta) {
    // Populate the data internally
    for (final chat in delta.upserted) {
      _chats[chat.id] = chat;
    }
    for (final entry in delta.members.entries) {
      _members[entry.key] = List.unmodifiable(entry.value);
    }
    for (final chatId in delta.removed) {
      _chats.remove(chatId);
      _members.remove(chatId);
    }

    final order = delta.order;
    if (order != null) {
      _order = order;
      _isLoaded = true;
    }

    // Emit changes (see `watchChat` and `watchMembers`)
    if (delta.upserted.isNotEmpty || delta.removed.isNotEmpty) {
      _chatChanges.add({...delta.upserted.map((c) => c.id), ...delta.removed});
    }
    if (delta.members.isNotEmpty || delta.removed.isNotEmpty) {
      _memberChanges.add({...delta.members.keys, ...delta.removed});
    }

    if (order != null) {
      _orderChanges.add(order);
    }
  }
}
