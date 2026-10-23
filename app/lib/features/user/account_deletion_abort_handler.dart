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

final _log = Logger('AccountDeletionAbortHandler');

/// Tells the user that the account deletion was aborted, which the delete
/// dialog can't do once the deletion went on in the background, and records
/// that they were told.
///
/// The account still exists then, and this device keeps working.
class AccountDeletionAbortHandler extends StatefulWidget {
  const AccountDeletionAbortHandler({super.key, required this.child});

  final Widget child;

  @override
  State<AccountDeletionAbortHandler> createState() =>
      _AccountDeletionAbortHandlerState();
}

class _AccountDeletionAbortHandlerState
    extends State<AccountDeletionAbortHandler> {
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final userCubit = context.read<UserCubit>();
    if (userCubit.state.accountDeletionAborted) {
      // The snackbar needs a scaffold, which the first frame builds.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _report(userCubit);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return BlocListener<UserCubit, UiUser>(
      listenWhen: (previous, current) =>
          !previous.accountDeletionAborted && current.accountDeletionAborted,
      listener: (context, _) => _report(context.read<UserCubit>()),
      child: widget.child,
    );
  }

  void _report(UserCubit userCubit) {
    showSnackBarStandalone(
      (loc) => loc.accountDeletionAbortHandler_notice,
      tone: .danger,
    );
    unawaited(
      userCubit.accountDeletionAbortReported().catchError((
        Object error,
        StackTrace stackTrace,
      ) {
        _log.severe(
          'Failed to record that the aborted deletion was reported',
          error,
          stackTrace,
        );
      }),
    );
  }
}
