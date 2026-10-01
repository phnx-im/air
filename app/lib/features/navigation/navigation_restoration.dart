// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_state.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:uuid/uuid.dart';

/// Persists a reduced [HomeNavigationState] (tab and open chat) in the
/// platform's state-restoration bucket, so a process killed for memory reopens
/// on the same screen.
class NavigationRestoration {
  NavigationRestoration(this._manager) {
    _manager.addListener(_claimAndReplay);
    _claimAndReplay();
  }

  final RestorationManager _manager;
  RestorationBucket? _root;
  RestorationBucket? _bucket;
  HomeNavigationState? _restoredHome;
  NavigationState? _lastState;
  bool _disposed = false;

  @visibleForTesting
  static const bucketId = 'navigation';
  @visibleForTesting
  static const lastHomeKey = 'lastHome';

  /// Returns and clears the restored home state, so a later re-login doesn't
  /// reapply a stale restore.
  HomeNavigationState? takeRestoredHome() {
    final home = _restoredHome;
    _restoredHome = null;
    return home;
  }

  /// Only persisted while logged in, so a restore never leaks a chat across
  /// accounts.
  void persist(NavigationState state) {
    _lastState = state;
    if (state is IntroState) _restoredHome = null;
    final bucket = _bucket;
    if (bucket == null) return;
    switch (state) {
      case HomeState(:final home):
        bucket.write(lastHomeKey, _encodeHome(home));
      case IntroState():
        bucket.remove<Object>(lastHomeKey);
    }
  }

  void dispose() {
    _disposed = true;
    _manager.removeListener(_claimAndReplay);
    _bucket?.dispose();
    _bucket = null;
  }

  Future<void> _claimBucket() async {
    final root = await _manager.rootBucket;
    if (_disposed || identical(root, _root)) return;
    _root = root;
    _bucket?.dispose();
    _bucket = root?.claimChild(bucketId, debugOwner: this);
    _restoredHome ??= _decodeHome(_bucket?.read<Object>(lastHomeKey));
  }

  // (Re-)claims the bucket and writes the last known state into it.
  void _claimAndReplay() => unawaited(
    _claimBucket().then((_) {
      if (_lastState case final state?) persist(state);
    }),
  );
}

Map<String, Object?> _encodeHome(HomeNavigationState home) => {
  'tab': home.activeTab.name,
  'chatId': home.chatId?.uuid.toString(),
  'chatOpen': home.chatOpen,
};

HomeNavigationState? _decodeHome(Object? value) {
  if (value is! Map) return null;
  final tab = HomeTab.values.firstWhere(
    (t) => t.name == value['tab'],
    orElse: () => HomeTab.chats,
  );
  final chatId = switch (value['chatId']) {
    final String s => UuidValue.fromString(s),
    _ => null,
  };
  return HomeNavigationState(
    activeTab: tab,
    chatId: chatId != null ? ChatId(uuid: chatId) : null,
    chatOpen: value['chatOpen'] == true,
  );
}
