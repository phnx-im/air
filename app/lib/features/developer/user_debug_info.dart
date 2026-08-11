// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/developer/developer_fields.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart' show Icons, SnackBar, Tooltip;
import 'package:flutter/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:intl/intl.dart';

/// The debug info of the loaded user, and a way to read it again.
typedef UserDebugInfoState = ({
  UserDebugInfo? info,
  Object? error,
  VoidCallback refresh,
});

/// Loads the debug info for [user], reloading it on
/// [UserDebugInfoState.refresh].
///
/// The info stays null while it loads, and while there is no user, so a host
/// renders the rest of its page right away rather than behind one spinner.
UserDebugInfoState useUserDebugInfo(User? user) {
  final refreshKey = useState(0);
  final snapshot = useFuture(
    useMemoized(() => user?.userDebugInfo(), [user, refreshKey.value]),
  );

  return (
    info: snapshot.data,
    error: snapshot.error,
    refresh: () => refreshKey.value++,
  );
}

/// The tasks the client runs on a timer, each with a trigger that runs it now.
/// Its own card, being the one run of unbounded length.
class TimedTasksCard extends StatelessWidget {
  const TimedTasksCard({
    required this.user,
    required this.tasks,
    required this.onTriggered,
    super.key,
  });

  final User user;
  final List<TimedTaskDebugInfo> tasks;

  /// Called once a trigger has run, so the host can re-read the schedule.
  final VoidCallback onTriggered;

  @override
  Widget build(BuildContext context) {
    return DeveloperCard(
      caption: 'Timed tasks (${tasks.length})',
      children: [
        for (final task in tasks)
          DeveloperInfoRow(
            label: task.name,
            value:
                '${_formatDateTime(task.scheduledAt.toLocal())}  '
                '(${_formatRelative(task.scheduledAt)})',
            trailing: _TriggerButton(onPressed: () => _trigger(task)),
          ),
      ],
    );
  }

  Future<void> _trigger(TimedTaskDebugInfo task) async {
    try {
      await user.triggerTimedTask(task.id);
      showSnackBarStandalone(
        (loc) => SnackBar(
          content: Text('Triggered ${task.name}'),
          duration: const Duration(seconds: 2),
        ),
      );
    } catch (error) {
      showSnackBarStandalone(
        (loc) => SnackBar(
          content: Text('Failed to trigger ${task.name}: $error'),
          duration: const Duration(seconds: 3),
        ),
      );
    }
    onTriggered();
  }
}

String _formatDateTime(DateTime dt) {
  return DateFormat('yyyy-MM-dd HH:mm:ss').format(dt);
}

String _formatRelative(DateTime dt) {
  final diff = dt.toUtc().difference(DateTime.now().toUtc());
  final abs = diff.abs();
  final future = diff.isNegative == false;
  String magnitude;
  if (abs.inSeconds < 60) {
    magnitude = '${abs.inSeconds}s';
  } else if (abs.inMinutes < 60) {
    magnitude = '${abs.inMinutes}m';
  } else if (abs.inHours < 24) {
    magnitude = '${abs.inHours}h ${abs.inMinutes.remainder(60)}m';
  } else {
    magnitude = '${abs.inDays}d ${abs.inHours.remainder(24)}h';
  }
  return future ? 'in $magnitude' : '$magnitude ago';
}

class _TriggerButton extends StatelessWidget {
  const _TriggerButton({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Tooltip(
      message: 'Run now',
      child: ButtonIcon(
        variant: ButtonIconVariant.plain,
        size: ButtonIconSize.s32,
        hitTargetSize: S.s40,
        iconWidget: Icon(
          Icons.play_arrow,
          size: 20,
          color: palette.text.primary,
        ),
        onPressed: onPressed,
      ),
    );
  }
}
