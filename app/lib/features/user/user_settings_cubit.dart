// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Provides the user settings to the app, across logins.
///
/// The Rust cubit is bound to a user, but this wrapper is provided app-wide and
/// therefore also exists before login. It starts detached and reports the
/// default settings. [attach] loads the persisted settings and swaps in a cubit
/// for the user, [detach] drops it again on logout.
class UserSettingsCubit implements StateStreamableSource<UserSettings> {
  final StreamController<UserSettings> _states =
      StreamController<UserSettings>.broadcast();

  UserSettingsCubitBase? _impl;
  StreamSubscription<UserSettings>? _implStates;
  UserSettings _detachedState = const UserSettings();

  /// The cubit of the logged in user. Only call this after [attach].
  UserSettingsCubitBase get impl => _impl!;

  /// Whether [attach] has completed and [impl] is available.
  bool get isAttached => _impl != null;

  /// Binds this wrapper to `user` and loads the persisted settings.
  Future<void> attach({required User user}) async {
    _dropImpl();
    var loaded = await loadUserSettings(user: user);
    if (_detachedState.isDeveloper) {
      // Carry over a developer flag that was toggled before login.
      loaded = loaded.copyWith(isDeveloper: true);
    }

    final impl = UserSettingsCubitBase(user: user, initial: loaded);
    _impl = impl;
    _implStates = impl.stream().listen(_states.add);
    // The stream only reports changes, so emit the loaded state for listeners
    // that are already subscribed.
    _states.add(loaded);
  }

  /// Unbinds this wrapper from the user and falls back to the default settings.
  void detach() {
    _dropImpl();
    _detachedState = const UserSettings();
    _states.add(_detachedState);
  }

  @override
  Future<void> close() async {
    _dropImpl();
    await _states.close();
  }

  @override
  bool get isClosed => _states.isClosed;

  @override
  UserSettings get state => _impl?.state ?? _detachedState;

  @override
  Stream<UserSettings> get stream => _states.stream;

  void _dropImpl() {
    unawaited(_implStates?.cancel());
    _implStates = null;
    _impl?.close();
    _impl = null;
  }

  // Cubit methods

  Future<void> setLocale({required String value}) =>
      _impl!.setLocale(value: value);

  Future<void> setInterfaceScale({required double value}) =>
      _impl!.setInterfaceScale(value: value);

  Future<void> setSidebarWidth({required double value}) =>
      _impl!.setSidebarWidth(value: value);

  Future<void> setSendOnEnter({required bool value}) =>
      _impl!.setSendOnEnter(value: value);

  Future<void> setReadReceipts({required bool value}) =>
      _impl!.setReadReceipts(value: value);

  Future<void> setIsDeveloper({required bool value}) async {
    final impl = _impl;
    if (impl == null) {
      // There is no user to persist to yet, so keep the flag in memory. It is
      // carried over by [attach].
      _detachedState = _detachedState.copyWith(isDeveloper: value);
      _states.add(_detachedState);
      return;
    }
    await impl.setIsDeveloper(value: value);
  }

  Future<void> setDefaultEmojiSkinTone({required int value}) =>
      _impl!.setDefaultEmojiSkinTone(value: value);
}
