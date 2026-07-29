// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core_client.dart';
import 'package:air/platform/method_channel.dart';

class BackgroundService {
  final CoreClient _coreClient = CoreClient();
  Timer? _timer;

  void start({bool runImmediately = false}) {
    if (runImmediately) _performTask();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => _performTask());
  }

  void stop() {
    _timer?.cancel();
  }

  void _performTask() async {
    // Set the badge count on macOS
    if (Platform.isMacOS) {
      // Make sure the user is logged in
      if (_coreClient.maybeUser case final user?) {
        final count = await user.globalUnreadMessagesCount;
        await setBadgeCount(count);
      }
    }
  }
}
