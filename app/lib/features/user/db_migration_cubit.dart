// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';

final _log = Logger('DbMigrationCubit');

/// Whether the startup splash shows that the database is being updated.
///
/// Latches: once the indicator is up it stays up for the rest of the splash,
/// so it does not blink away when the migrations finish but loading the user
/// carries on.
class DbMigrationCubit extends Cubit<bool> {
  DbMigrationCubit({
    Stream<DbMigrationState>? stream,
    this._dismissSplash = dismissSplashScreen,
  }) : super(false) {
    _subscription = (stream ?? createDbMigrationStream()).listen(
      _onState,
      onError: (error, stackTrace) {
        _log.severe('Migration stream failed: $error', error, stackTrace);
      },
    );
  }

  /// Migrations shorter than this never flash an indicator.
  static const _showDelay = Duration(milliseconds: 400);

  final Future<void> Function() _dismissSplash;
  late final StreamSubscription<DbMigrationState> _subscription;
  Timer? _showTimer;

  @override
  Future<void> close() async {
    _showTimer?.cancel();
    await _subscription.cancel();
    await super.close();
  }

  void _onState(DbMigrationState migrationState) {
    switch (migrationState) {
      case DbMigrationState.running:
        if (state || _showTimer != null) return;
        _showTimer = Timer(_showDelay, _show);
      case DbMigrationState.idle:
        _showTimer?.cancel();
        _showTimer = null;
    }
  }

  void _show() {
    _showTimer = null;
    if (isClosed) return;
    emit(true);
    // The Android splash screen is kept on top until this call, so it would
    // otherwise cover the indicator entirely.
    unawaited(_dismissSplash());
  }
}
