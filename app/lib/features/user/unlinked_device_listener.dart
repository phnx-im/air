// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';

final _log = Logger('UnlinkedDeviceListener');

/// Tears this device down once another device of the user unlinks it or
/// deletes the account.
///
/// Deletes the local client database and drops the loaded user, which lands the
/// app back on the welcome screen. Nothing is registered or re-created. After
/// an account deletion, a notice tells the user why the device was reset.
class UnlinkedDeviceHandler extends StatefulWidget {
  const UnlinkedDeviceHandler({super.key, required this.child});

  final Widget child;

  @override
  State<UnlinkedDeviceHandler> createState() => _UnlinkedDeviceHandlerState();
}

class _UnlinkedDeviceHandlerState extends State<UnlinkedDeviceHandler> {
  bool _tearDownStarted = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reason = context.read<UserCubit>().state.unlinkReason;
    if (reason != null) {
      _startTearDown(context.read<CoreClient>(), reason);
    }
  }

  @override
  Widget build(BuildContext context) {
    return BlocListener<UserCubit, UiUser>(
      // The stream can catch up between the dependency check and listener
      // initialization. The teardown guard makes repeated states safe.
      listenWhen: (_, current) => current.unlinkReason != null,
      listener: (context, state) {
        final reason = state.unlinkReason;
        if (reason != null) {
          _startTearDown(context.read<CoreClient>(), reason);
        }
      },
      child: widget.child,
    );
  }

  void _startTearDown(CoreClient coreClient, UiUnlinkReason reason) {
    if (_tearDownStarted) {
      return;
    }
    _tearDownStarted = true;
    unawaited(_tearDown(coreClient, reason));
  }

  Future<void> _tearDown(CoreClient coreClient, UiUnlinkReason reason) async {
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
    switch (reason) {
      case UiUnlinkReason.unlinked:
        break;
      case UiUnlinkReason.accountDeleted:
        showSnackBarStandalone(
          (loc) => loc.unlinkedDevice_accountDeleted,
          tone: .danger,
        );
    }
  }
}
