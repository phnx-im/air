// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/l10n/language_options.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

part 'user_session_cubit.freezed.dart';

@freezed
sealed class UserSessionState with _$UserSessionState {
  const UserSessionState._();

  const factory UserSessionState({
    User? user,

    /// True once we know there is no session (never logged in, or logged out).
    /// False at startup while the default user is still loading.
    @Default(false) bool loggedOut,
  }) = _UserSessionState;

  /// The currently logged in user
  User? get activeUser => loggedOut ? null : user;
}

class UserSessionCubit extends Cubit<UserSessionState> {
  UserSessionCubit({
    required this._coreClient,
    required this._navigationCubit,
    required this._userSettingsCubit,
    required this._appLocaleCubit,
  }) : super(const UserSessionState()) {
    _subscription = _coreClient.userStream.asyncMap(_onUserChange).listen(null);
  }

  final CoreClient _coreClient;
  final NavigationCubit _navigationCubit;
  final UserSettingsCubit _userSettingsCubit;
  final AppLocaleCubit _appLocaleCubit;

  late final StreamSubscription<void> _subscription;

  static const _tearDownGrace = Duration(milliseconds: 500);

  @override
  Future<void> close() async {
    await _subscription.cancel();
    await super.close();
  }

  Future<void> _onUserChange(User? user) =>
      user != null ? _openSession(user) : _closeSession();

  Future<void> _openSession(User user) async {
    // Attach settings before emitting
    await _userSettingsCubit.attach(user: user);
    await _syncLocale();
    emit(UserSessionState(user: user));

    final navigationState = _navigationCubit.state;
    if (navigationState is! HomeState && !navigationState.isCreatingAccount) {
      _navigationCubit.openHome();
    }
    unawaited(_coreClient.refreshPushToken());
  }

  Future<void> _closeSession() async {
    if (state.user == null) {
      // Startup with no default user => no session to tear down
      emit(const UserSessionState(loggedOut: true));
      return;
    }

    _navigationCubit.openIntro();
    _userSettingsCubit.detach();

    // Keep the user subtree alive until the intro transition finishes.
    emit(UserSessionState(user: state.user!, loggedOut: true));
    unawaited(
      Future.delayed(_tearDownGrace).then((_) {
        if (!isClosed && state.loggedOut) {
          emit(const UserSessionState(loggedOut: true));
        }
      }),
    );
  }

  Future<void> _syncLocale() async {
    final userLocale = localeFromTag(_userSettingsCubit.state.locale);
    final appLocale = _appLocaleCubit.state;
    if (userLocale != null) {
      // Sync UI locale with persisted user preference.
      _appLocaleCubit.setLocale(userLocale);
    } else if (appLocale != null) {
      // Persist pre-user selection once the user exists.
      await _userSettingsCubit.setLocale(value: localeToTag(appLocale));
    }
  }
}
