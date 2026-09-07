// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter/widgets.dart';
import 'package:logging/logging.dart';

final _log = Logger('AppLifecycle');

/// Translates Flutter lifecycle events into [AppState] transitions.
///
/// On iOS, entering the background also requests additional background time
/// to stop the outbound service gracefully and updates the badge count.
class AppLifecycleHandler with WidgetsBindingObserver {
  AppLifecycleHandler({required this._coreClient});

  final CoreClient _coreClient;
  final StreamController<AppState> _appStateController =
      StreamController<AppState>.broadcast();
  int? _backgroundTaskId;

  Stream<AppState> get appStateStream => _appStateController.stream;

  void start() {
    WidgetsBinding.instance.addObserver(this);
  }

  void stop() {
    WidgetsBinding.instance.removeObserver(this);
    _appStateController.close();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    super.didChangeAppLifecycleState(state);
    _onStateChanged(state);
  }

  Future<void> _onStateChanged(AppLifecycleState state) async {
    // Detect background transitions

    if (DeviceType.isDesktop && state == AppLifecycleState.inactive) {
      // On desktop platforms, the inactive state is entered when the user
      // switches to another app. In that case, we want to treat it as
      // background state.
      _appStateController.sink.add(AppState.desktopBackground);
      return;
    }
    if (DeviceType.isPhone && state == AppLifecycleState.paused) {
      // On mobile platforms, the paused state is entered when the app
      // is closed. In that case, we want to treat it as background state.
      _appStateController.sink.add(AppState.mobileBackground);

      // iOS only
      if (Platform.isIOS) {
        // Request additional background time until the outbound service is
        // stopped
        await _prepareForBackground();
        // only set the badge count if the user is logged in
        if (_coreClient.maybeUser case final user?) {
          final count = await user.globalUnreadMessagesCount;
          await setBadgeCount(count);
        }
      }
      return;
    }

    // Detect foreground transitions

    if (state == AppLifecycleState.resumed) {
      _appStateController.sink.add(AppState.foreground);
      unawaited(_coreClient.refreshPushToken());
    }
  }

  Future<void> _prepareForBackground() async {
    if (!Platform.isIOS) return;

    final startedAt = DateTime.now();
    _log.info('prepareForBackground: requesting background task');
    _backgroundTaskId = await beginBackgroundTask();
    _log.info(
      'prepareForBackground: background task started id=$_backgroundTaskId',
    );

    // Ask the coreclient to stop the outbound service gracefully
    final user = _coreClient.maybeUser;
    if (user == null) {
      _log.info('prepareForBackground: no user, ending background task');
      await endBackgroundTask(_backgroundTaskId);
      _backgroundTaskId = null;
      return;
    }

    try {
      await user.prepareForBackground();
    } finally {
      final elapsed = DateTime.now().difference(startedAt);
      await endBackgroundTask(_backgroundTaskId);
      _log.info(
        'prepareForBackground: ended background task after ${elapsed.inMilliseconds}ms',
      );
      _backgroundTaskId = null;
    }
  }
}
