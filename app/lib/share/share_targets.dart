// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/platform/method_channel.dart';
import 'package:logging/logging.dart';
import 'package:uuid/uuid.dart';

final _log = Logger('ShareTargets');

/// Tail of the chain running the OS share target updates one at a time.
///
/// An update loads chats before it reaches the OS, so an unawaited publish
/// can still be in flight when logout clears the targets. Chaining the
/// updates keeps such a publish from recreating the targets after the
/// clear.
Future<void> _updates = Future.value();

Future<void> _enqueueUpdate(
  String description,
  Future<void> Function() update,
) {
  _updates = _updates.then((_) async {
    try {
      await update();
    } catch (e, stacktrace) {
      // Broader than PlatformException on purpose. The user can unload
      // while an update is in flight, and the resulting error must not
      // break the chain or escape the unawaited future.
      _log.severe('Failed to $description', e, stacktrace);
    }
  });
  return _updates;
}

/// Publishes the most recently used chats to the system share sheet as
/// direct share targets (Android sharing shortcuts) and withdraws targets
/// whose chat can no longer be shared into.
Future<void> publishShareTarget({
  required UserCubit userCubit,
  required ChatsRepository chatsRepository,
  required ChatId chatId,
}) {
  if (Platform.isAndroid) {
    return _enqueueUpdate(
      'publish share targets',
      () => _publishShareTarget(userCubit, chatsRepository, chatId),
    );
  } else if (Platform.isIOS) {
    return _enqueueUpdate(
      'donate share target',
      () => _donateIOSShareTarget(userCubit, chatId),
    );
  } else {
    return Future.value();
  }
}

Future<void> _publishShareTarget(
  UserCubit userCubit,
  ChatsRepository chatsRepository,
  ChatId chatId,
) async {
  final target = await loadShareTarget(
    userCubit: userCubit.impl,
    chatId: chatId,
  );
  if (target == null) {
    return;
  }
  await platform.invokeMethod('publishShareShortcuts', {
    'targets': [_encodeTarget(target)],
    'usedChatId': chatId.uuid.toString(),
  });
  await _removeStaleShareTargets(chatsRepository, target);
}

/// Withdraws published share targets whose chat is gone or can no longer be
/// shared into (e.g. deleted, left, blocked or not yet accepted chats).
Future<void> _removeStaleShareTargets(
  ChatsRepository chatsRepository,
  UiShareTarget published,
) async {
  if (!chatsRepository.isLoaded) {
    return;
  }
  final ids = await platform.invokeMethod<List<Object?>>('getShareShortcutIds');
  final publishedId = published.chatId.uuid.toString();
  final stale = <String>[];
  for (final id in (ids ?? const []).whereType<String>()) {
    if (id == publishedId) {
      continue;
    }
    final chatId = _parseChatId(id);
    final chat = chatId == null ? null : chatsRepository.getChat(chatId);
    if (chat == null || !canShareIntoChat(chat)) {
      stale.add(id);
    }
  }
  if (stale.isNotEmpty) {
    await platform.invokeMethod('removeShareShortcuts', {'ids': stale});
  }
}

/// Whether the chat still takes shared content.
bool canShareIntoChat(UiChatDetails chat) =>
    chat.status == const UiChatStatus.active() &&
    chat.chatType is! UiChatType_PendingConnection;

/// The share target ids are the uuids of the published chats.
ChatId? _parseChatId(String id) {
  try {
    return ChatId(uuid: UuidValue.withValidation(id));
  } on FormatException {
    return null;
  }
}

Future<void> _donateIOSShareTarget(UserCubit userCubit, ChatId chatId) async {
  final target = await loadShareTarget(
    userCubit: userCubit.impl,
    chatId: chatId,
  );
  if (target == null) {
    return;
  }
  await platform.invokeMethod('donateShareTarget', _encodeTarget(target));
}

/// Removes all published share targets from the OS.
Future<void> clearShareTargets() {
  if (!Platform.isAndroid && !Platform.isIOS) {
    return Future.value();
  }
  return _enqueueUpdate(
    'clear share targets',
    () => platform.invokeMethod('clearShareTargets'),
  );
}

Map<String, dynamic> _encodeTarget(UiShareTarget target) => {
  'chatId': target.chatId.uuid.toString(),
  'title': target.title,
  'isGroup': target.isGroup,
  'picture': target.picture,
};
