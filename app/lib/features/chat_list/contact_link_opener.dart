// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';

/// Passes the username of an opened contact link to [show] once home is on
/// screen.
class ContactLinkOpener {
  ContactLinkOpener({required this._navigationCubit, required this._show}) {
    _subscription = _navigationCubit.stream.listen(_onNavigation);
  }

  final NavigationCubit _navigationCubit;
  final void Function(UiUsername username) _show;
  late final StreamSubscription<NavigationState> _subscription;

  UiUsername? _pending;

  void open(UiUsername username) {
    if (_navigationCubit.state is HomeState) {
      _show(username);
    } else {
      _pending = username;
    }
  }

  void _onNavigation(NavigationState state) {
    final username = _pending;
    if (username == null || state is! HomeState) return;
    _pending = null;
    _show(username);
  }

  Future<void> dispose() => _subscription.cancel();
}
