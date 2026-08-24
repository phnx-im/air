// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/platform/method_channel.dart';
import 'package:logging/logging.dart';
import 'package:uuid/uuid.dart';

final _log = Logger('ShareTargets');

/// Number of chats published as direct share targets
const _maxShareTargets = 8;

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
///
/// [chatId] is the chat the user just interacted with, if any. It is
/// reported to the OS as shortcut usage, which is what the share sheet
/// ranks the direct share targets by.
Future<void> publishShareTargets({
  required UserCubit userCubit,
  ChatId? chatId,
}) {
  if (!Platform.isAndroid) {
    return Future.value();
  }
  return _enqueueUpdate(
    'publish share targets',
    () => _publishShareTargets(userCubit, chatId),
  );
}

Future<void> _publishShareTargets(
  UserCubit userCubit,
  ChatId? usedChatId,
) async {
  final targets = await loadShareTargets(
    userCubit: userCubit.impl,
    limit: _maxShareTargets,
  );
  await platform.invokeMethod('publishShareShortcuts', {
    'targets': targets.map(_encodeTarget).toList(),
    'usedChatId': usedChatId?.uuid.toString(),
  });
  await _removeStaleShareTargets(userCubit, targets);
}

/// Withdraws published share targets whose chat is gone or can no longer be
/// shared into (e.g. deleted or left chats).
///
/// Since nothing else removes them, they would stay selectable in the share
/// sheet until logout. Only the OS knows which targets are still published,
/// so their ids are fetched from it and checked one by one.
Future<void> _removeStaleShareTargets(
  UserCubit userCubit,
  List<UiShareTarget> published,
) async {
  final ids = await platform.invokeMethod<List<Object?>>('getShareShortcutIds');
  final publishedIds = published
      .map((target) => target.chatId.uuid.toString())
      .toSet();
  final stale = <String>[];
  for (final id in (ids ?? const []).whereType<String>()) {
    if (publishedIds.contains(id)) {
      continue;
    }
    final chatId = _parseChatId(id);
    final target = chatId == null
        ? null
        : await loadShareTarget(userCubit: userCubit.impl, chatId: chatId);
    if (target == null) {
      stale.add(id);
    }
  }
  if (stale.isNotEmpty) {
    await platform.invokeMethod('removeShareShortcuts', {'ids': stale});
  }
}

/// The share target ids are the uuids of the published chats.
ChatId? _parseChatId(String id) {
  try {
    return ChatId(uuid: UuidValue.withValidation(id));
  } on FormatException {
    return null;
  }
}

/// Donates the chat to the share sheet suggestions (iOS
/// `INSendMessageIntent`).
Future<void> donateShareTarget({
  required UserCubit userCubit,
  required ChatId chatId,
}) {
  if (!Platform.isIOS) {
    return Future.value();
  }
  return _enqueueUpdate(
    'donate share target',
    () => _donateShareTarget(userCubit, chatId),
  );
}

Future<void> _donateShareTarget(UserCubit userCubit, ChatId chatId) async {
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
