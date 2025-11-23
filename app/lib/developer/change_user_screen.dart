// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:provider/provider.dart';

class ChangeUserScreen extends StatefulWidget {
  const ChangeUserScreen({super.key});

  @override
  State<ChangeUserScreen> createState() => _ChangeUserScreenState();
}

class _ChangeUserScreenState extends State<ChangeUserScreen> {
  Future<List<UiClientRecord>>? _clientRecords;

  @override
  void initState() {
    super.initState();
    loadClientRecords();
  }

  void loadClientRecords() async {
    final clientRecords = User.loadClientRecords(dbPath: await dbPath());
    setState(() {
      _clientRecords = clientRecords;
    });
  }

  @override
  Widget build(BuildContext context) {
    return ChangeUserScreenView(clientRecords: _clientRecords);
  }
}

const _maxDesktopWidth = 800.0;

class ChangeUserScreenView extends StatelessWidget {
  const ChangeUserScreenView({this.clientRecords, super.key});

  final Future<List<UiClientRecord>>? clientRecords;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Change User'),
        toolbarHeight: isPointer() ? 100 : null,
        leading: const AppBarBackButton(),
      ),
      body: Center(
        child: Container(
          constraints: isPointer()
              ? const BoxConstraints(maxWidth: _maxDesktopWidth)
              : null,
          child: _ClientRecords(clientRecords: clientRecords),
        ),
      ),
    );
  }
}

class _ClientRecords extends StatelessWidget {
  const _ClientRecords({this.clientRecords});

  final Future<List<UiClientRecord>>? clientRecords;

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<UiClientRecord>>(
      future: clientRecords,
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          return _ClientRecordsList(snapshot.data!);
        } else if (snapshot.hasError) {
          return const Text('Error loading contacts');
        }
        return const CircularProgressIndicator();
      },
    );
  }
}

class _ClientRecordsList extends StatelessWidget {
  const _ClientRecordsList(this.clientRecords);

  final List<UiClientRecord> clientRecords;

  @override
  Widget build(BuildContext context) {
    final ownClientId = context.select(
      (LoadableUserCubit cubit) => cubit.state.user?.userId,
    );

    return Center(
      child: ListView(
        children: clientRecords.map((record) {
          final isCurrentUser = record.userId == ownClientId;
          final currentUserSuffix = isCurrentUser ? " (current)" : "";

          final textColor = isCurrentUser
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
              offset: const Offset(0, Spacings.xxs),
              child: UserAvatar(userId: record.userId, size: Spacings.xl),
            ),
            title: Text(record.userProfile.displayName + currentUserSuffix),
            subtitle: Text(
              "Domain: ${record.userId.domain}\n"
              "ID: ${record.userId.uuid.toString()}\n"
              "Created: ${record.createdAt}\n"
              "Fully registered: ${record.isFinished ? "yes" : "no"}",
            ),
            onTap: !isCurrentUser
                ? () {
                    final coreClient = context.read<CoreClient>();
                    coreClient.logout();
                    coreClient.loadUser(userId: record.userId);
                  }
                : null,
          );
        }).toList(),
      ),
    );
  }
}
