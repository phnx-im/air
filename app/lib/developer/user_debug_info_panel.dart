// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/typography/monospace.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:intl/intl.dart';

/// Debug info panel for the currently logged-in user.
///
/// Note: Strings in this widget are not localized.
class UserDebugInfoPanel extends HookWidget {
  const UserDebugInfoPanel({required this.user, super.key});

  final User user;

  @override
  Widget build(BuildContext context) {
    final snapshot = useFuture(useMemoized(() => user.userDebugInfo()));
    final colors = CustomColorScheme.of(context);

    return switch (snapshot) {
      AsyncSnapshot(hasData: true, :final data) => _UserDebugInfoBody(
        info: data!,
      ),
      AsyncSnapshot(hasError: true, :final error) => Center(
        child: Padding(
          padding: const EdgeInsets.all(Spacings.s),
          child: Text(
            error.toString(),
            style: TextStyle(
              fontSize: BodyFontSize.small1.size,
              color: colors.text.secondary,
            ),
          ),
        ),
      ),
      _ => Center(
        child: SizedBox(
          width: 16,
          height: 16,
          child: CircularProgressIndicator(
            strokeWidth: 2,
            valueColor: AlwaysStoppedAnimation<Color>(colors.text.primary),
          ),
        ),
      ),
    };
  }
}

class _UserDebugInfoBody extends StatelessWidget {
  const _UserDebugInfoBody({required this.info});

  final UserDebugInfo info;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionHeader('User'),
        _InfoCard(
          children: [
            _InfoRow(label: 'User ID', value: info.userId, monospace: true),
          ],
        ),
        const SizedBox(height: Spacings.s),
        const _SectionHeader('Privacy Pass Tokens'),
        _InfoCard(
          children: [
            _InfoRow(
              label: 'Add Username',
              value: info.addUsernameTokenCount.toString(),
            ),
            _InfoRow(
              label: 'Invite Code',
              value: info.invitationCodeTokenCount.toString(),
            ),
          ],
        ),
        const SizedBox(height: Spacings.s),
        _SectionHeader('Timed Tasks (${info.timedTasks.length})'),
        _InfoCard(
          children: [
            for (final task in info.timedTasks)
              _InfoRow(
                label: task.name,
                value:
                    '${_formatDateTime(task.scheduledAt.toLocal())}  (${_formatRelative(task.scheduledAt)})',
              ),
          ],
        ),
      ],
    );
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
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader(this.title);

  final String title;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Spacings.xxs),
      child: Text(
        title.toUpperCase(),
        style: TextStyle(
          fontSize: BodyFontSize.small2.size,
          fontWeight: FontWeight.bold,
          color: colors.text.tertiary,
        ),
      ),
    );
  }
}

class _InfoCard extends StatelessWidget {
  const _InfoCard({required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    if (children.isEmpty) {
      return Container(
        decoration: BoxDecoration(
          color: colors.backgroundBase.secondary,
          borderRadius: BorderRadius.circular(12),
        ),
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.s,
          vertical: Spacings.xs,
        ),
        child: Text(
          '—',
          style: TextStyle(
            fontSize: BodyFontSize.small1.size,
            color: colors.text.tertiary,
          ),
        ),
      );
    }
    return Container(
      decoration: BoxDecoration(
        color: colors.backgroundBase.secondary,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (int i = 0; i < children.length; i++) ...[
            children[i],
            if (i < children.length - 1)
              Divider(
                height: 1,
                indent: Spacings.s,
                color: colors.separator.secondary,
              ),
          ],
        ],
      ),
    );
  }
}

class _InfoRow extends StatelessWidget {
  const _InfoRow({
    required this.label,
    required this.value,
    this.monospace = false,
  });

  final String label;
  final String value;
  final bool monospace;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    var valueStyle = TextStyle(
      fontSize: BodyFontSize.small1.size,
      color: colors.text.primary,
    );
    if (monospace) {
      valueStyle = valueStyle.withSystemMonospace();
    }

    return InkWell(
      onTap: () {
        Clipboard.setData(ClipboardData(text: value));
        showSnackBarStandalone(
          (loc) => SnackBar(
            content: Text('Copied $label'),
            duration: const Duration(seconds: 2),
          ),
        );
      },
      borderRadius: BorderRadius.circular(12),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.s,
          vertical: Spacings.xs,
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 200,
              child: Text(
                label,
                style: TextStyle(
                  fontSize: BodyFontSize.small1.size,
                  color: colors.text.tertiary,
                ),
              ),
            ),
            const SizedBox(width: Spacings.xs),
            Expanded(child: Text(value, style: valueStyle)),
          ],
        ),
      ),
    );
  }
}
