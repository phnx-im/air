// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/typography/monospace.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// Debug info about a chat group.
///
/// Note: Strings in this class are not localized.
class ChatDebugInfoView extends HookWidget {
  const ChatDebugInfoView({
    required this.title,
    required this.debugInfo,
    super.key,
  });

  final String title;
  final Future<GroupDebugInfo> debugInfo;

  @override
  Widget build(BuildContext context) {
    final snapshot = useFuture(debugInfo);
    final colors = CustomColorScheme.of(context);

    return AppScaffold(
      title: title,
      child: switch (snapshot) {
        AsyncSnapshot(hasData: true, :final data) => _GroupDebugInfoBody(
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
      },
    );
  }
}

class _GroupDebugInfoBody extends StatelessWidget {
  const _GroupDebugInfoBody({required this.info});

  final GroupDebugInfo info;

  @override
  Widget build(BuildContext context) {
    final sortedMembers = info.members.entries.toList()
      ..sort((a, b) => a.key.compareTo(b.key));

    return ListView(
      children: [
        const SizedBox(height: Spacings.s),
        const _SectionHeader('Overview'),
        _InfoCard(
          children: [
            _InfoRow(label: 'Group ID', value: info.groupId, monospace: true),
            _InfoRow(label: 'Epoch', value: info.epoch.toString()),
            _InfoRow(label: 'Ciphersuite', value: info.ciphersuite),
            _ChipListRow(label: 'Protocol Versions', values: info.versions),
            _InfoRow(
              label: 'Own Leaf Index',
              value: info.ownLeafIndex.toString(),
            ),
            _InfoRow(
              label: 'Self Updated At',
              value: info.selfUpdatedAt ?? '—',
            ),
            _InfoRow(
              label: 'Pending Proposals',
              value: info.pendingProposals.toString(),
            ),
            _InfoRow(
              label: 'Pending Commit',
              value: info.hasPendingCommit ? 'yes' : 'no',
            ),
          ],
        ),
        if (info.groupDataCbor != null) ...[
          const SizedBox(height: Spacings.s),
          const _SectionHeader('Group Data'),
          _GroupDataCard(cbor: info.groupDataCbor!),
        ],
        if (info.requiredCapabilities != null) ...[
          const SizedBox(height: Spacings.s),
          const _SectionHeader('Required Capabilities'),
          _RequiredCapabilitiesCard(caps: info.requiredCapabilities!),
        ],
        const SizedBox(height: Spacings.s),
        _SectionHeader('Members (${sortedMembers.length})'),
        for (final entry in sortedMembers) ...[
          const SizedBox(height: Spacings.xs),
          _MemberCard(
            leafIndex: entry.key,
            caps: entry.value,
            isOwn: entry.key == info.ownLeafIndex,
          ),
        ],
        const SizedBox(height: Spacings.l),
      ],
    );
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
          fontWeight: FontWeight.w600,
          color: colors.text.tertiary,
          letterSpacing: 0.5,
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
    return Container(
      decoration: BoxDecoration(
        color: colors.backgroundBase.secondary,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
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
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
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
              width: 140,
              child: Text(
                label,
                style: TextStyle(
                  fontSize: BodyFontSize.small1.size,
                  color: colors.text.tertiary,
                ),
              ),
            ),
            Expanded(child: Text(value, style: valueStyle)),
          ],
        ),
      ),
    );
  }
}

class _GroupDataCard extends StatelessWidget {
  const _GroupDataCard({required this.cbor});

  final String cbor;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return InkWell(
      onTap: () {
        Clipboard.setData(ClipboardData(text: cbor));
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Copied Group Data'),
            duration: Duration(seconds: 2),
          ),
        );
      },
      borderRadius: BorderRadius.circular(12),
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.all(Spacings.s),
        decoration: BoxDecoration(
          color: colors.backgroundBase.secondary,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Text(
          cbor,
          style: TextStyle(
            fontSize: BodyFontSize.small2.size,
            color: colors.text.primary,
          ).withSystemMonospace(),
        ),
      ),
    );
  }
}

class _RequiredCapabilitiesCard extends StatelessWidget {
  const _RequiredCapabilitiesCard({required this.caps});

  final RequiredDebugCapabilities caps;

  @override
  Widget build(BuildContext context) {
    return _InfoCard(
      children: [
        _ChipListRow(label: 'Extensions', values: caps.extensionTypes),
        _ChipListRow(label: 'Proposals', values: caps.proposalTypes),
        _ChipListRow(label: 'Credentials', values: caps.credentialTypes),
      ],
    );
  }
}

class _MemberCard extends StatelessWidget {
  const _MemberCard({
    required this.leafIndex,
    required this.caps,
    required this.isOwn,
  });

  final int leafIndex;
  final DebugCapabilities caps;
  final bool isOwn;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Container(
      decoration: BoxDecoration(
        color: isOwn
            ? colors.backgroundBase.quaternary
            : colors.backgroundBase.secondary,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: Spacings.s,
              vertical: Spacings.xs,
            ),
            child: Row(
              children: [
                Text(
                  'Leaf $leafIndex',
                  style: TextStyle(
                    fontSize: BodyFontSize.small1.size,
                    fontWeight: FontWeight.w600,
                    color: colors.text.primary,
                  ),
                ),
                if (isOwn) ...[
                  const SizedBox(width: Spacings.xxs),
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 6,
                      vertical: 2,
                    ),
                    decoration: BoxDecoration(
                      color: colors.accent.primary.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      'self',
                      style: TextStyle(
                        fontSize: BodyFontSize.small2.size,
                        color: colors.accent.primary,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                  ),
                ],
              ],
            ),
          ),
          Divider(
            height: 1,
            indent: Spacings.s,
            color: colors.separator.secondary,
          ),
          _InfoRow(label: 'User ID', value: caps.userId, monospace: true),
          Divider(
            height: 1,
            indent: Spacings.s,
            color: colors.separator.secondary,
          ),
          _InfoRow(label: 'Display Name', value: caps.displayName),
          Divider(
            height: 1,
            indent: Spacings.s,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Versions', values: caps.versions),
          Divider(
            height: 1,
            indent: Spacings.s,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Ciphersuites', values: caps.ciphersuites),
          Divider(
            height: 1,
            indent: Spacings.s,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Extensions', values: caps.extensions),
          Divider(
            height: 1,
            indent: Spacings.s,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Proposals', values: caps.proposals),
        ],
      ),
    );
  }
}

class _ChipListRow extends StatelessWidget {
  const _ChipListRow({required this.label, required this.values});

  final String label;
  final List<String> values;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    if (values.isEmpty) {
      return _InfoRow(label: label, value: '—');
    }

    return InkWell(
      onTap: () {
        Clipboard.setData(ClipboardData(text: values.join(', ')));
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
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
              width: 140,
              child: Text(
                label,
                style: TextStyle(
                  fontSize: BodyFontSize.small1.size,
                  color: colors.text.tertiary,
                ),
              ),
            ),
            Expanded(
              child: Wrap(
                spacing: Spacings.xxs,
                runSpacing: Spacings.xxs,
                children: [for (final value in values) _Chip(value)],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip(this.label);

  final String label;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: colors.fill.primary,
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        label,
        style: TextStyle(
          fontSize: BodyFontSize.small2.size,
          color: colors.text.secondary,
        ).withSystemMonospace(),
      ),
    );
  }
}
