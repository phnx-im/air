// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:air/core/core.dart';

part 'loadable_user_cubit.freezed.dart';

/// A user that can be loading or unloading, or being fully loaded or unloaded.
@freezed
sealed class LoadableUser with _$LoadableUser {
  const LoadableUser._();

  /// Initial state before or in process of loading the user
  const factory LoadableUser.loading() = LoadingUser;

  /// The user was fully unloaded
  const factory LoadableUser.unloaded() = UnloadedUser;

  /// The user has been loaded
  ///
  /// If loading was successful, the [user] will be non-null.
  const factory LoadableUser.loaded(User user) = LoadedUser;

  /// The user is currently unloading
  const factory LoadableUser.unloading(User user) = UnloadingUser;

  User? get loadedUser => switch (this) {
    LoadedUser(:final user) => user,
    _ => null,
  };

  User? get maybeUser => switch (this) {
    LoadedUser(:final user) => user,
    UnloadingUser(:final user) => user,
    _ => null,
  };

  bool get isLoading => this is LoadingUser;

  LoadableUser apply(User? newUser) => switch (this) {
    LoadingUser() => newUser == null ? this : LoadableUser.loaded(newUser),
    UnloadedUser() => newUser == null ? this : LoadedUser(newUser),
    LoadedUser(user: final previousUser) =>
      newUser == null ? UnloadingUser(previousUser) : LoadedUser(newUser),
    UnloadingUser() => newUser == null ? this : LoadedUser(newUser),
  };
}

/// Observe the [User] state as [LoadableUser] initialized from a [User] stream
///
/// Can be plugged into a [BlocProvider].
class LoadableUserCubit implements StateStreamableSource<LoadableUser> {
  LoadableUserCubit(Stream<User?> stream) {
    // forward the stream to an internal broadcast stream
    _subscription = stream.listen((user) {
      _state.value = _state.value.apply(user);
    });

    _state.addListener(_handleUpdate);
  }

  final ValueNotifier<LoadableUser> _state = ValueNotifier(
    const LoadableUser.loading(),
  );
  bool _isClosed = false;
  final StreamController<LoadableUser> _controller =
      StreamController.broadcast();
  late final StreamSubscription<User?> _subscription;

  @override
  FutureOr<void> close() async {
    if (!_isClosed) return;
    _isClosed = true;

    _state.removeListener(_handleUpdate);
    await _subscription.cancel();
    await _controller.close();
    _state.dispose();
  }

  @override
  bool get isClosed => _isClosed;

  @override
  LoadableUser get state => _state.value;

  @override
  Stream<LoadableUser> get stream => _controller.stream;

  void finishUnloading() {
    if (_state.value is UnloadingUser) {
      _state.value = const LoadableUser.unloaded();
    }
  }

  void _handleUpdate() {
    if (!_isClosed) {
      _controller.add(_state.value);
    }
  }
}
