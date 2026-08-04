// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';

final _log = Logger('UnlinkedDeviceListener');

/// Tears this device down once another device of the user unlinks it.
///
/// Deletes the local client database and drops the loaded user, which lands the
/// app back on the welcome screen. Nothing is registered or re-created: the
/// account still exists and lives on the user's remaining devices.
class UnlinkedDeviceListener extends StatefulWidget {
  const UnlinkedDeviceListener({super.key, required this.child});

  final Widget child;

  @override
  State<UnlinkedDeviceListener> createState() => _UnlinkedDeviceListenerState();
}

class _UnlinkedDeviceListenerState extends State<UnlinkedDeviceListener> {
  bool _tearDownStarted = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (context.read<UserCubit>().state.accountUnlinked) {
      _startTearDown(context.read<CoreClient>());
    }
  }

  @override
  Widget build(BuildContext context) {
    return BlocListener<UserCubit, UiUser>(
      // The flag is terminal, so only the transition into it is interesting.
      listenWhen: (previous, current) =>
          !previous.accountUnlinked && current.accountUnlinked,
      listener: (context, _) => _startTearDown(context.read<CoreClient>()),
      child: widget.child,
    );
  }

  void _startTearDown(CoreClient coreClient) {
    if (_tearDownStarted) {
      return;
    }
    _tearDownStarted = true;
    unawaited(_tearDown(coreClient));
  }

  Future<void> _tearDown(CoreClient coreClient) async {
    _log.warning(
      'This device was unlinked by another device of this user. '
      'Stopping background work and deleting the local client database.',
    );
    try {
      await coreClient.deleteCurrentDatabase();
    } catch (error, stackTrace) {
      _log.severe(
        'Failed to tear down the local client database after being unlinked',
        error,
        stackTrace,
      );
      // Drop the user anyway: staying signed in on a device the user unlinked
      // is worse than leaving data behind.
      coreClient.logout();
    }
  }
}
