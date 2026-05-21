// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/components/app_scaffold.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/monospace.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// Debug info about a chat group.
///
/// Note: Strings in this class are not localized.
class ChatDebugInfoView extends HookWidget {
  const ChatDebugInfoView({
    required this.title,
    required this.loadDebugInfo,
    required this.onUpdateGroup,
    required this.onRequestResync,
    required this.onEraseLocalChat,
    super.key,
  });

  final String title;
  final Future<GroupDebugInfo> Function() loadDebugInfo;
  final AsyncCallback onUpdateGroup;
  final VoidCallback onRequestResync;
  final VoidCallback onEraseLocalChat;

  @override
  Widget build(BuildContext context) {
    final debugInfoFuture = useState(useMemoized(loadDebugInfo));
    final snapshot = useFuture(debugInfoFuture.value);
    final colors = CustomColorScheme.of(context);

    return AppScaffold(
      title: title,
      child: switch (snapshot) {
        AsyncSnapshot(hasData: true, :final data) => _GroupDebugInfoBody(
          info: data!,
          onUpdateGroup: () async {
            await onUpdateGroup();
            debugInfoFuture.value = loadDebugInfo();
          },
          onRequestResync: onRequestResync,
          onEraseLocalChat: onEraseLocalChat,
        ),
        AsyncSnapshot(hasError: true, :final error) => Center(
          child: Padding(
            padding: const EdgeInsets.all(Spacing.px16),
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
  const _GroupDebugInfoBody({
    required this.info,
    required this.onUpdateGroup,
    required this.onRequestResync,
    required this.onEraseLocalChat,
  });

  final GroupDebugInfo info;
  final AsyncCallback onUpdateGroup;
  final VoidCallback onRequestResync;
  final VoidCallback onEraseLocalChat;

  @override
  Widget build(BuildContext context) {
    final sortedMembers = info.members.entries.toList()
      ..sort((a, b) => a.key.compareTo(b.key));

    return ListView(
      children: [
        const SizedBox(height: Spacing.px16),
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
            _InfoRow(label: 'Size', value: _formatBytes(info.sizeBytes)),
          ],
        ),
        const SizedBox(height: Spacing.px16),
        const _SectionHeader('Post-Quantum'),
        if (info.pq == null)
          const _InfoCard(
            children: [_InfoRow(label: 'Enabled', value: 'no')],
          )
        else
          _InfoCard(
            children: [
              const _InfoRow(label: 'Enabled', value: 'yes'),
              _InfoRow(
                label: 'Group ID',
                value: info.pq!.groupId,
                monospace: true,
              ),
              _InfoRow(label: 'Epoch', value: info.pq!.epoch.toString()),
              _InfoRow(label: 'Ciphersuite', value: info.pq!.ciphersuite),
              _InfoRow(
                label: 'Self Updated At',
                value: info.pq!.selfUpdatedAt ?? '—',
              ),
              _InfoRow(
                label: 'Pending Proposals',
                value: info.pq!.pendingProposals.toString(),
              ),
              _InfoRow(
                label: 'Pending Commit',
                value: info.pq!.hasPendingCommit ? 'yes' : 'no',
              ),
              _InfoRow(label: 'Size', value: _formatBytes(info.pq!.sizeBytes)),
            ],
          ),
        if (info.groupData != null) ...[
          const SizedBox(height: Spacing.px16),
          const _SectionHeader('Group Data'),
          _GroupDataInfoCard(data: info.groupData!),
        ],
        if (info.requiredCapabilities != null) ...[
          const SizedBox(height: Spacing.px16),
          const _SectionHeader('Required Capabilities'),
          _RequiredCapabilitiesCard(caps: info.requiredCapabilities!),
        ],
        const SizedBox(height: Spacing.px16),
        _SectionHeader('Members (${sortedMembers.length})'),
        for (final entry in sortedMembers) ...[
          const SizedBox(height: Spacing.px12),
          _MemberCard(
            leafIndex: entry.key,
            caps: entry.value,
            isOwn: entry.key == info.ownLeafIndex,
          ),
        ],
        const SizedBox(height: Spacing.px32),
        _UpdateGroupButton(onTapped: onUpdateGroup),
        const SizedBox(height: Spacing.px16),
        _RequestResyncButton(onTapped: onRequestResync),
        const SizedBox(height: Spacing.px16),
        _DeleteLocalChatButton(onTapped: onEraseLocalChat),
      ],
    );
  }
}

class _UpdateGroupButton extends HookWidget {
  const _UpdateGroupButton({required this.onTapped});

  final AsyncCallback onTapped;

  @override
  Widget build(BuildContext context) {
    final isRunning = useState(false);
    return AppButton(
      onPressed: () async {
        isRunning.value = true;
        try {
          await onTapped();
        } finally {
          isRunning.value = false;
        }
      },
      state: isRunning.value ? AppButtonState.inactive : AppButtonState.active,
      label: "Update group",
    );
  }
}

class _RequestResyncButton extends HookWidget {
  const _RequestResyncButton({required this.onTapped});

  final VoidCallback onTapped;

  @override
  Widget build(BuildContext context) {
    final isTapped = useState(false);
    return AppButton(
      onPressed: () {
        isTapped.value = true;
        onTapped();
      },
      tone: AppButtonTone.danger,
      state: isTapped.value ? AppButtonState.inactive : AppButtonState.active,
      label: "DANGER: Request resync",
    );
  }
}

class _DeleteLocalChatButton extends HookWidget {
  const _DeleteLocalChatButton({required this.onTapped});

  final VoidCallback onTapped;

  @override
  Widget build(BuildContext context) {
    final isTapped = useState(false);
    return AppButton(
      onPressed: () {
        isTapped.value = true;
        onTapped();
      },
      tone: AppButtonTone.danger,
      state: isTapped.value ? AppButtonState.inactive : AppButtonState.active,
      label: "DANGER: Delete local chat",
    );
  }
}

String _formatBytes(BigInt bytes) {
  final n = bytes.toInt();
  if (n < 1024) return '$n B';
  if (n < 1024 * 1024) return '${(n / 1024).toStringAsFixed(1)} KiB';
  if (n < 1024 * 1024 * 1024) {
    return '${(n / (1024 * 1024)).toStringAsFixed(2)} MiB';
  }
  return '${(n / (1024 * 1024 * 1024)).toStringAsFixed(2)} GiB';
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader(this.title);

  final String title;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Spacing.px8),
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

class _CardSectionHeader extends StatelessWidget {
  const _CardSectionHeader(this.title);

  final String title;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.px16,
        vertical: Spacing.px12,
      ),
      child: Text(
        title,
        style: TextStyle(
          fontSize: BodyFontSize.small2.size,
          fontWeight: FontWeight.w600,
          color: colors.text.tertiary,
          letterSpacing: 0.3,
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
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (int i = 0; i < children.length; i++) ...[
            children[i],
            if (i < children.length - 1)
              Divider(
                height: 1,
                indent: Spacing.px16,
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
          horizontal: Spacing.px16,
          vertical: Spacing.px12,
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
            Expanded(child: Text(value, style: valueStyle)),
          ],
        ),
      ),
    );
  }
}

class _GroupDataInfoCard extends StatelessWidget {
  const _GroupDataInfoCard({required this.data});

  final GroupDataDebugInfo data;

  @override
  Widget build(BuildContext context) {
    return _InfoCard(
      children: [
        _InfoRow(label: 'Legacy Title', value: data.legacyTitle ?? '—'),

        _InfoRow(
          label: 'Legacy Picture',
          value: data.legacyPicture ? 'yes' : 'no',
        ),

        if (data.encryptedTitle == null)
          const _InfoRow(label: 'Encrypted Title', value: '—'),

        if (data.encryptedTitle != null) ...[
          const _CardSectionHeader('Encrypted Title'),
          _InfoRow(
            label: 'Ciphertext',
            value: data.encryptedTitle!.ciphertext,
            monospace: true,
          ),
          _InfoRow(
            label: 'Nonce',
            value: data.encryptedTitle!.nonce,
            monospace: true,
          ),
          _InfoRow(
            label: 'AAD',
            value: data.encryptedTitle!.aad,
            monospace: true,
          ),
        ],

        if (data.externalGroupProfile == null)
          const _InfoRow(label: 'External Group Profile', value: '—'),

        if (data.externalGroupProfile != null) ...[
          const _CardSectionHeader('External Group Profile'),
          _InfoRow(
            label: 'Object ID',
            value: data.externalGroupProfile!.objectId,
            monospace: true,
          ),
          _InfoRow(
            label: 'Size',
            value: data.externalGroupProfile!.size.toString(),
          ),
          _InfoRow(
            label: 'Enc Alg',
            value: data.externalGroupProfile!.encAlg ?? '—',
          ),
          _InfoRow(
            label: 'Nonce',
            value: data.externalGroupProfile!.nonce,
            monospace: true,
          ),
          _InfoRow(
            label: 'AAD',
            value: data.externalGroupProfile!.aad,
            monospace: true,
          ),
          _InfoRow(
            label: 'Hash Alg',
            value: data.externalGroupProfile!.hashAlg,
          ),
          _InfoRow(
            label: 'Content Hash',
            value: data.externalGroupProfile!.contentHash,
            monospace: true,
          ),
        ],
      ],
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
              horizontal: Spacing.px16,
              vertical: Spacing.px12,
            ),
            child: Row(
              children: [
                Text(
                  'Leaf $leafIndex',
                  style: TextStyle(
                    fontSize: BodyFontSize.small1.size,
                    fontWeight: FontWeight.bold,
                    color: colors.text.primary,
                  ),
                ),
                if (isOwn) ...[
                  const SizedBox(width: Spacing.px8),
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
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                ],
              ],
            ),
          ),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _InfoRow(label: 'User ID', value: caps.userId, monospace: true),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _InfoRow(label: 'Display Name', value: caps.displayName),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Versions', values: caps.versions),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Ciphersuites', values: caps.ciphersuites),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Extensions', values: caps.extensions),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _ChipListRow(label: 'Proposals', values: caps.proposals),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _ChipListRow(
            label: 'App Components',
            values: caps.appData?.components ?? [],
          ),
          Divider(
            height: 1,
            indent: Spacing.px16,
            color: colors.separator.secondary,
          ),
          _ChipListRow(
            label: 'Air Component',
            values: [
              if (caps.appData?.airComponent?.features.encryptedGroupProfiles ==
                  true)
                'encrypted_group_profiles',
              if (caps
                      .appData
                      ?.airComponent
                      ?.features
                      .emptyConnectionGroupAttributes ==
                  true)
                'empty_connection_group_attributes',
            ],
          ),
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
          horizontal: Spacing.px16,
          vertical: Spacing.px12,
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
            Expanded(
              child: Wrap(
                spacing: Spacing.px8,
                runSpacing: Spacing.px8,
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
