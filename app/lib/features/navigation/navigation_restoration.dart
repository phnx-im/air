// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:convert';

import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:uuid/uuid.dart';

/// Persists a reduced [HomeNavigationState] (tab and open chat) through
/// Flutter's platform state-restoration channel, so an Android process
/// killed for memory (task still in Recents) reopens on the same screen.
///
/// Must sit under a [MaterialApp]/[WidgetsApp] with `restorationScopeId`
/// set, so a [RestorationScope] exists to attach to.
class NavigationRestorationScope extends StatefulWidget {
  const NavigationRestorationScope({
    required this.navigationCubit,
    required this.child,
    super.key,
  });

  final NavigationCubit navigationCubit;
  final Widget child;

  @override
  State<NavigationRestorationScope> createState() =>
      _NavigationRestorationScopeState();
}

class _NavigationRestorationScopeState extends State<NavigationRestorationScope>
    with RestorationMixin {
  final RestorableStringN _lastHome = RestorableStringN(null);
  StreamSubscription<NavigationState>? _subscription;

  @override
  String? get restorationId => 'navigation';

  @override
  void initState() {
    super.initState();
    _subscription = widget.navigationCubit.stream.listen(_onNavigationChange);
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _lastHome.dispose();
    super.dispose();
  }

  @override
  void restoreState(RestorationBucket? oldBucket, bool initialRestore) {
    registerForRestoration(_lastHome, 'lastHome');
    if (initialRestore) {
      final restored = _decode(_lastHome.value);
      if (restored != null) {
        widget.navigationCubit.setRestoredHome(restored);
      }
    }
  }

  // Only persisted while logged in: an Intro transition (logout, account
  // switch) clears it so a later restore never leaks a chat across accounts.
  void _onNavigationChange(NavigationState state) {
    switch (state) {
      case HomeState(:final home):
        _lastHome.value = _encode(home);
      case IntroState():
        _lastHome.value = null;
    }
  }

  static String _encode(HomeNavigationState home) => jsonEncode({
    'tab': home.activeTab.name,
    'chatId': home.chatId?.uuid.toString(),
    'chatOpen': home.chatOpen,
  });

  static HomeNavigationState? _decode(String? value) {
    if (value == null) return null;
    try {
      final json = jsonDecode(value) as Map<String, dynamic>;
      final tab = HomeTab.values.firstWhere(
        (t) => t.name == json['tab'],
        orElse: () => HomeTab.chats,
      );
      final chatIdString = json['chatId'] as String?;
      return HomeNavigationState(
        activeTab: tab,
        chatId: chatIdString != null
            ? ChatId(uuid: UuidValue.fromString(chatIdString))
            : null,
        chatOpen: json['chatOpen'] as bool? ?? false,
      );
    } catch (_) {
      return null;
    }
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
