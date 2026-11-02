// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/user/user_session_cubit.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/features/developer/developer_fields.dart';
import 'package:air/features/user/avatar.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

final _log = Logger('ChangeUserModal');

/// Opens the user picker on the surface the device calls for.
Future<void> showChangeUser(BuildContext context) => showAppModal<void>(
  context: context,
  builder: (_) => const ChangeUserModal(),
);

class ChangeUserModal extends StatelessWidget {
  const ChangeUserModal({super.key, this.loadClientRecords});

  /// Overrides loading the client records from the database (used in tests).
  final Future<List<UiClientRecord>> Function()? loadClientRecords;

  @override
  Widget build(BuildContext context) {
    return ModalScaffold(
      title: 'Change User',
      onDismiss: () => Navigator.of(context).pop(),
      child: ModalBody(
        child: _ClientRecordsList(loadClientRecords: loadClientRecords),
      ),
    );
  }
}

class _ClientRecordsList extends HookWidget {
  const _ClientRecordsList({this.loadClientRecords});

  final Future<List<UiClientRecord>> Function()? loadClientRecords;

  @override
  Widget build(BuildContext context) {
    final ownClientRecordId = context.select(
      (UserSessionCubit cubit) => cubit.state.activeUser?.clientRecordId,
    );

    // Bumped after a deletion to load the records again.
    final generation = useState(0);
    final clientRecordsFut = useMemoized(
      () => (loadClientRecords ?? _loadClientRecords)(),
      [generation.value],
    );
    final records = useFuture(clientRecordsFut);

    final data = records.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    // The host scrolls this, so the records are a plain column.
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: data.map((record) {
        final isCurrentRecord = record.clientRecordId == ownClientRecordId;
        final currentUserSuffix = isCurrentRecord ? " (current)" : "";

        final textColor = isCurrentRecord
            ? Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.38)
            : null;

        final tile = ListTile(
          // The modal's inset places the row, so the tile adds none.
          contentPadding: EdgeInsets.zero,
          titleAlignment: ListTileTitleAlignment.top,
          titleTextStyle: Theme.of(context).textTheme.bodyMedium
              ?.copyWith(color: textColor, fontWeight: .bold),
          subtitleTextStyle: Theme.of(context).textTheme.bodySmall
              ?.copyWith(color: textColor),
          leading: Transform.translate(
            offset: const Offset(0, S.s8),
            child: UserAvatar(profile: record.userProfile, size: S.s48),
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

        if (isCurrentRecord) {
          return tile;
        }

        return Row(
          children: [
            Expanded(child: tile),
            const SizedBox(width: S.s16),
            DeveloperRowButton(
              icon: AppIconType.trash,
              tooltip: 'Delete account',
              iconColor: SemanticPalette.of(context).function.danger,
              onPressed: () => _confirmDelete(
                context,
                record,
                onDeleted: () => generation.value++,
              ),
            ),
          ],
        );
      }).toList(),
    );
  }
}

Future<List<UiClientRecord>> _loadClientRecords() async =>
    User.loadClientRecords(dbPath: await dbPath());

void _confirmDelete(
  BuildContext context,
  UiClientRecord record, {
  required VoidCallback onDeleted,
}) {
  final coreClient = context.read<CoreClient>();
  showDialog<bool>(
    context: context,
    builder: (_) => ConfirmDialog(
      title: 'Delete account',
      message:
          'Delete ${record.userProfile.displayName} and its data from this '
          'device? The account on the server stays.',
      cancel: 'Cancel',
      confirm: 'Delete',
      destructive: true,
      onConfirm: () async {
        try {
          await coreClient.deleteUnloadedDatabase(record.clientRecordId);
        } catch (error, stackTrace) {
          _log.severe('Failed to delete account', error, stackTrace);
          showSnackBarStandalone(
            (_) => 'Failed to delete account: $error',
            tone: .danger,
          );
        }
        // We reload after a failure as well, since the record can be gone
        // even when removing a database file failed.
        if (context.mounted) onDeleted();
      },
    ),
  );
}
