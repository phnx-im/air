// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter/services.dart';
import 'package:logging/logging.dart';

final _log = Logger('ShareTargets');

/// Donates the chat to the share sheet suggestions (iOS
/// `INSendMessageIntent`).
Future<void> donateShareTarget({
  required UserCubit userCubit,
  required ChatId chatId,
}) async {
  if (!Platform.isIOS) {
    return;
  }
  try {
    final target = await loadShareTarget(
      userCubit: userCubit.impl,
      chatId: chatId,
    );
    if (target == null) {
      return;
    }
    await platform.invokeMethod('donateShareTarget', _encodeTarget(target));
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to donate share target: '${e.message}'", e, stacktrace);
  }
}

/// Removes all donated share targets from the OS.
Future<void> clearShareTargets() async {
  if (!Platform.isIOS) {
    return;
  }
  try {
    await platform.invokeMethod('clearShareTargets');
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to clear share targets: '${e.message}'", e, stacktrace);
  }
}

Map<String, dynamic> _encodeTarget(UiShareTarget target) => {
  'chatId': target.chatId.uuid.toString(),
  'title': target.title,
  'isGroup': target.isGroup,
  'picture': target.picture,
};
