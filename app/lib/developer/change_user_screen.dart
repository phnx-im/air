// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

const _maxDesktopWidth = 800.0;

class ChangeUserScreen extends StatelessWidget {
  const ChangeUserScreen({super.key, this.clientRecords});

  /// Overrides loading the client records from the database (used in tests).
  final Future<List<UiClientRecord>>? clientRecords;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        clipBehavior: Clip.none,
        title: const Text('Change User'),
        toolbarHeight: DeviceType.isDesktop ? 100 : null,
        leading: const AppBarBackButton(),
      ),
      body: Center(
        child: Container(
          constraints: DeviceType.isDesktop
              ? const BoxConstraints(maxWidth: _maxDesktopWidth)
              : null,
          child: _ClientRecordsList(clientRecords: clientRecords),
        ),
      ),
    );
  }
}

class _ClientRecordsList extends HookWidget {
  const _ClientRecordsList({this.clientRecords});

  final Future<List<UiClientRecord>>? clientRecords;

  @override
  Widget build(BuildContext context) {
    final ownClientRecordId = context.select(
      (LoadableUserCubit cubit) => cubit.state.loadedUser?.clientRecordId,
    );

    final clientRecordsFut = useMemoized(
      () =>
          clientRecords ??
          dbPath().then((dbPath) => User.loadClientRecords(dbPath: dbPath)),
    );
    final records = useFuture(clientRecordsFut);

    final data = records.data;
    if (data == null) return const CircularProgressIndicator();

    return Center(
      child: ListView(
        children: data.map((record) {
          final isCurrentRecord = record.clientRecordId == ownClientRecordId;
          final currentUserSuffix = isCurrentRecord ? " (current)" : "";

          final textColor = isCurrentRecord
              ? Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.38)
              : null;

          return ListTile(
            titleAlignment: ListTileTitleAlignment.top,
            titleTextStyle: Theme.of(context).textTheme.bodyMedium?.copyWith(
              color: textColor,
              fontWeight: FontWeight.bold,
            ),
            subtitleTextStyle: Theme.of(
              context,
            ).textTheme.bodySmall?.copyWith(color: textColor),
            leading: Transform.translate(
              offset: const Offset(0, Spacing.px8),
              child: UserAvatar(
                profile: record.userProfile,
                size: Spacing.px48,
              ),
            ),
            title: Text(record.userProfile.displayName + currentUserSuffix),
            subtitle: Text(
              "Domain: ${record.userId.domain}\n"
              "User ID: ${record.userId.uuid.toString()}\n"
              "Record: ${record.clientRecordId.toString()}\n"
              "Created: ${record.createdAt}\n"
              "Fully registered: ${record.isFinished ? "yes" : "no"}",
            ),
            onTap: !isCurrentRecord
                ? () {
                    final coreClient = context.read<CoreClient>();
                    coreClient.logout();
                    coreClient.loadUser(clientRecordId: record.clientRecordId);
                  }
                : null,
          );
        }).toList(),
      ),
    );
  }
}
