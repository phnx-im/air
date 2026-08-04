// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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
class UnlinkedDeviceListener extends StatelessWidget {
  const UnlinkedDeviceListener({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return BlocListener<UserCubit, UiUser>(
      // The flag is terminal, so only the transition into it is interesting.
      listenWhen: (previous, current) =>
          !previous.accountUnlinked && current.accountUnlinked,
      listener: (context, _) => _tearDown(context.read<CoreClient>()),
      child: child,
    );
  }

  Future<void> _tearDown(CoreClient coreClient) async {
    _log.warning(
      'This device was unlinked by another device of this user. '
      'Deleting the local client database.',
    );
    try {
      await coreClient.deleteUserDatabase();
    } catch (error, stackTrace) {
      _log.severe(
        'Failed to delete the local client database after being unlinked',
        error,
        stackTrace,
      );
      // Drop the user anyway: staying signed in on a device the user unlinked
      // is worse than leaving data behind.
      coreClient.logout();
    }
  }
}
