// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/platform/method_channel.dart';
import 'package:logging/logging.dart';

final _log = Logger('ShareTargetPublisher');

/// Publishes the chats the user sends to as direct share targets of the system
/// share sheet (Android sharing shortcuts, iOS `INSendMessageIntent`
/// donations), keeps them in sync with the chats and withdraws them on
/// [dispose].
class ShareTargetPublisher {
  ShareTargetPublisher({required this._chatsRepository}) {
    _changes = _chatsRepository.watchChanges().listen(_onChatsChanged);
    if (Platform.isAndroid) {
      unawaited(_enqueue('reconcile share targets', _reconcile));
    }
  }

  final ChatsRepository _chatsRepository;

  // State

  /// Published Android shortcuts, keyed by chat.
  ///
  /// A null target means the shortcut predates this session, so next use
  /// republishes it.
  final _published = <ChatId, _ShareTarget?>{};
  late final StreamSubscription<Set<ChatId>> _changes;

  /// Runs the OS updates one at a time.
  ///
  /// An update reads the chat before it reaches the OS, so this keeps a
  /// publish from landing after dispose.
  Future<void> _updates = Future.value();
  bool _disposed = false;

  Future<void> _enqueue(String description, Future<void> Function() update) {
    _updates = _updates.then((_) async {
      if (_disposed) return;
      try {
        await update();
      } catch (e, stacktrace) {
        _log.severe('Failed to $description', e, stacktrace);
      }
    });
    return _updates;
  }

  /// The user just sent a new message into [chatId].
  void reportUsed(ChatId chatId) {
    if (Platform.isAndroid) {
      unawaited(_enqueue('publish share target', () => _publish(chatId)));
    } else if (Platform.isIOS) {
      unawaited(_enqueue('donate share target', () => _donate(chatId)));
    }
  }

  /// Withdraws all published share targets.
  ///
  /// Called when the provider tree of the logged-in user is torn down, that
  /// is, on logout and unlink (via `finishUnloading`) and on a direct account
  /// switch. The targets carry chat names and avatars and the OS keeps them
  /// until the app removes them, so they must not outlive the account.
  ///
  /// Not called on process death or hot restart. The shortcuts then survive in
  /// the OS and the next session adopts them in [_reconcile].
  Future<void> dispose() async {
    _disposed = true;
    await _changes.cancel();
    // Wait for in-flight updates, then clear regardless of _disposed.
    await _updates;
    _published.clear();
    if (Platform.isAndroid || Platform.isIOS) {
      try {
        await platform.invokeMethod('clearShareTargets');
      } catch (e, stacktrace) {
        _log.severe('Failed to clear share targets', e, stacktrace);
      }
    }
  }

  /// Keeps published shortcuts current and withdraws the ones whose chat can
  /// no longer be shared into.
  void _onChatsChanged(Set<ChatId> ids) {
    if (!Platform.isAndroid) return;
    for (final chatId in ids) {
      if (!_published.containsKey(chatId)) continue;
      final chat = _chatsRepository.getChat(chatId);
      if (chat == null || !chat.canShareInto) {
        unawaited(_enqueue('withdraw share target', () => _withdraw(chatId)));
      } else if (_published[chatId] != _shareTarget(chat)) {
        unawaited(_enqueue('refresh share target', () => _publish(chatId)));
      }
    }
  }

  // Android specific

  Future<void> _publish(ChatId chatId) async {
    final chat = _chatsRepository.getChat(chatId);
    if (chat == null || !chat.canShareInto) return;
    final target = _shareTarget(chat);
    if (_published.containsKey(chatId) && _published[chatId] == target) {
      await platform.invokeMethod('reportShareShortcutUsed', {
        'chatId': chatId.uuid.toString(),
      });
      return;
    }
    // Publishing also reports the shortcut as used.
    await platform.invokeMethod(
      'publishShareShortcut',
      _encodeTarget(chatId, target),
    );
    _published[chatId] = target;
  }

  Future<void> _withdraw(ChatId chatId) async {
    _published.remove(chatId);
    await platform.invokeMethod('removeShareShortcuts', {
      'ids': [chatId.uuid.toString()],
    });
  }

  /// Shortcuts left over from a previous run: drops the ones whose chat is
  /// gone or unshareable and adopts the rest with an unknown target, so their
  /// next use republishes them.
  Future<void> _reconcile() async {
    if (!_chatsRepository.isLoaded) {
      await _chatsRepository.watchOrder().firstWhere(
        (_) => _chatsRepository.isLoaded,
      );
    }
    final ids =
        await platform.invokeMethod<List<Object?>>('getShareShortcutIds') ??
        const [];
    final stale = <String>[];
    for (final id in ids.whereType<String>()) {
      final chatId = ChatIdExtension.fromString(id);
      final chat = chatId == null ? null : _chatsRepository.getChat(chatId);
      if (chat == null || !chat.canShareInto) {
        stale.add(id);
      } else {
        _published[chatId!] = null;
      }
    }
    if (stale.isNotEmpty) {
      await platform.invokeMethod('removeShareShortcuts', {'ids': stale});
    }
  }

  // iOS specific

  Future<void> _donate(ChatId chatId) async {
    final chat = _chatsRepository.getChat(chatId);
    if (chat == null || !chat.canShareInto) return;
    await platform.invokeMethod(
      'donateShareTarget',
      _encodeTarget(chatId, _shareTarget(chat)),
    );
  }
}

/// What the OS shows for a chat as a share target.
typedef _ShareTarget = ({String title, bool isGroup, ImageData? picture});

_ShareTarget _shareTarget(UiChatDetails chat) => (
  title: chat.title,
  isGroup: chat.chatType is UiChatType_Group,
  picture: chat.picture,
);

Map<String, dynamic> _encodeTarget(ChatId chatId, _ShareTarget target) => {
  'chatId': chatId.uuid.toString(),
  'title': target.title,
  'isGroup': target.isGroup,
  'picture': target.picture?.data,
};
