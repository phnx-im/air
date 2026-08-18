// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/developer/developer_fields.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart'
    show CircularProgressIndicator, MaterialPageRoute;
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// The way into the debug view from a chat's details, present only while
/// developer mode is on.
///
/// We take the chat from the cubit rather than the host, so the same row
/// serves a group and a contact.
class ChatDebugInfoRow extends StatelessWidget {
  const ChatDebugInfoRow({super.key});

  @override
  Widget build(BuildContext context) {
    final developerMode = context.select(
      (UserSettingsCubit cubit) => cubit.state.developerMode,
    );
    final chat = context.select((ChatDetailsCubit cubit) => cubit.state.chat);

    if (!developerMode || chat == null) {
      return const SizedBox.shrink();
    }

    // The row owns the gap above it, so a details page without one carries no
    // spacer either.
    return Padding(
      padding: const EdgeInsets.only(top: S.s24),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          const DeveloperCaption('Developer'),
          ListRow(
            tokens: ListRowTokens.current,
            fill: SemanticPalette.of(context).backgroundBase.secondary,
            label: 'Debug info',
            trailing: const AppIcon.chevronRight(size: developerRowIconSize),
            onTap: () => showChatDebugInfo(context, chat),
          ),
        ],
      ),
    );
  }
}

/// Pushes the debug view for [chat], wired to the cubits above [context].
///
/// Pageless, so it stays out of the navigation state.
void showChatDebugInfo(BuildContext context, UiChatDetails chat) {
  final chatDetailsCubit = context.read<ChatDetailsCubit>();
  final userCubit = context.read<UserCubit>();
  Navigator.of(context).push(
    MaterialPageRoute(
      builder: (context) => ChatDebugInfoView(
        title: chat.title,
        loadDebugInfo: () => chatDetailsCubit.chatDebugInfo(),
        onUpdateGroup: () => chatDetailsCubit.updateKey(),
        onUpdateApqGroup: () => chatDetailsCubit.updateApqKey(),
        onRequestResync: () => chatDetailsCubit.requestResync(),
        onEraseLocalChat: () => userCubit.devEraseChat(chat.id),
      ),
    ),
  );
}

/// Debug info about a chat group.
class ChatDebugInfoView extends HookWidget {
  const ChatDebugInfoView({
    required this.title,
    required this.loadDebugInfo,
    required this.onUpdateGroup,
    required this.onUpdateApqGroup,
    required this.onRequestResync,
    required this.onEraseLocalChat,
    super.key,
  });

  final String title;
  final Future<GroupDebugInfo> Function() loadDebugInfo;
  final AsyncCallback onUpdateGroup;
  final AsyncCallback onUpdateApqGroup;
  final VoidCallback onRequestResync;
  final VoidCallback onEraseLocalChat;

  @override
  Widget build(BuildContext context) {
    final debugInfoFuture = useState(useMemoized(loadDebugInfo));
    final snapshot = useFuture(debugInfoFuture.value);
    final palette = SemanticPalette.of(context);

    return AppScaffold(
      title: title,
      // Pushed over the whole window, so the back button sits in the window's
      // own corner.
      reserveWindowControls: Chrome.windowControlsFloat,
      child: switch (snapshot) {
        AsyncSnapshot(hasData: true, :final data) => _GroupDebugInfoBody(
          info: data!,
          onUpdateGroup: () async {
            await onUpdateGroup();
            debugInfoFuture.value = loadDebugInfo();
          },
          onUpdateApqGroup: () async {
            await onUpdateApqGroup();
            debugInfoFuture.value = loadDebugInfo();
          },
          onRequestResync: onRequestResync,
          onEraseLocalChat: onEraseLocalChat,
        ),
        AsyncSnapshot(hasError: true, :final error) => Center(
          child: Padding(
            padding: const EdgeInsets.all(S.s16),
            child: Text(
              error.toString(),
              style: typeScale.body.s.style(color: palette.text.secondary),
            ),
          ),
        ),
        _ => Center(
          child: SizedBox(
            width: 16,
            height: 16,
            child: CircularProgressIndicator(
              strokeWidth: StrokeWidth.px2,
              valueColor: AlwaysStoppedAnimation<Color>(palette.text.primary),
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
    required this.onUpdateApqGroup,
    required this.onRequestResync,
    required this.onEraseLocalChat,
  });

  final GroupDebugInfo info;
  final AsyncCallback onUpdateGroup;
  final AsyncCallback onUpdateApqGroup;
  final VoidCallback onRequestResync;
  final VoidCallback onEraseLocalChat;

  @override
  Widget build(BuildContext context) {
    final sortedMembers = info.members.entries.toList()
      ..sort((a, b) => a.key.compareTo(b.key));

    return Column(
      crossAxisAlignment: .stretch,
      spacing: S.s16,
      children: [
        DeveloperCard(
          caption: 'Overview',
          children: [
            DeveloperInfoRow(
              label: 'Group ID',
              value: info.groupId,
              monospace: true,
            ),
            DeveloperInfoRow(label: 'Epoch', value: info.epoch.toString()),
            DeveloperInfoRow(label: 'Ciphersuite', value: info.ciphersuite),
            _ChipListRow(label: 'Protocol Versions', values: info.versions),
            DeveloperInfoRow(
              label: 'Own Leaf Index',
              value: info.ownLeafIndex.toString(),
            ),
            DeveloperInfoRow(
              label: 'Self Updated At',
              value: info.selfUpdatedAt ?? '—',
            ),
            DeveloperInfoRow(
              label: 'Pending Proposals',
              value: info.pendingProposals.toString(),
            ),
            DeveloperInfoRow(
              label: 'Pending Commit',
              value: info.hasPendingCommit ? 'yes' : 'no',
            ),
            DeveloperInfoRow(
              label: 'Size',
              value: _formatBytes(info.sizeBytes),
            ),
          ],
        ),
        if (info.pq case final pq?)
          DeveloperCard(
            caption: 'Post-Quantum',
            children: [
              const DeveloperInfoRow(label: 'Enabled', value: 'yes'),
              DeveloperInfoRow(
                label: 'Group ID',
                value: pq.groupId,
                monospace: true,
              ),
              DeveloperInfoRow(label: 'Epoch', value: pq.epoch.toString()),
              DeveloperInfoRow(label: 'Ciphersuite', value: pq.ciphersuite),
              DeveloperInfoRow(
                label: 'Self Updated At',
                value: pq.selfUpdatedAt ?? '—',
              ),
              DeveloperInfoRow(
                label: 'Pending Proposals',
                value: pq.pendingProposals.toString(),
              ),
              DeveloperInfoRow(
                label: 'Pending Commit',
                value: pq.hasPendingCommit ? 'yes' : 'no',
              ),
              DeveloperInfoRow(
                label: 'Size',
                value: _formatBytes(pq.sizeBytes),
              ),
            ],
          )
        else
          const DeveloperCard(
            caption: 'Post-Quantum',
            children: [DeveloperInfoRow(label: 'Enabled', value: 'no')],
          ),
        if (info.groupData case final data?) _GroupDataCard(data: data),
        if (info.requiredCapabilities case final caps?)
          DeveloperCard(
            caption: 'Required Capabilities',
            children: [
              _ChipListRow(label: 'Extensions', values: caps.extensionTypes),
              _ChipListRow(label: 'Proposals', values: caps.proposalTypes),
              _ChipListRow(label: 'Credentials', values: caps.credentialTypes),
            ],
          ),
        Column(
          crossAxisAlignment: .stretch,
          spacing: S.s12,
          children: [
            DeveloperCaption('Members (${sortedMembers.length})'),
            for (final entry in sortedMembers)
              _MemberCard(
                leafIndex: entry.key,
                caps: entry.value,
                isOwn: entry.key == info.ownLeafIndex,
              ),
          ],
        ),
        const SizedBox(height: S.s16),
        _UpdateGroupButton(onTapped: onUpdateGroup, label: "Update group"),
        if (info.pq != null)
          _UpdateGroupButton(
            onTapped: onUpdateApqGroup,
            label: "Update APQ group",
          ),
        DeveloperCard(
          caption: 'Danger zone',
          children: [
            DeveloperDangerRow(
              label: 'Request resync',
              icon: AppIconType.refreshCcw,
              confirmMessage:
                  'Are you sure you want to request a resync of this group?',
              confirmLabel: 'Resync',
              onConfirm: onRequestResync,
            ),
            DeveloperDangerRow(
              label: 'Delete local chat',
              icon: AppIconType.trash,
              confirmMessage:
                  'Are you sure you want to delete this chat locally?',
              confirmLabel: 'Delete',
              onConfirm: onEraseLocalChat,
            ),
          ],
        ),
      ],
    );
  }
}

class _UpdateGroupButton extends HookWidget {
  const _UpdateGroupButton({required this.label, required this.onTapped});

  final String label;
  final AsyncCallback onTapped;

  @override
  Widget build(BuildContext context) {
    final isRunning = useState(false);
    return Button(
      onPressed: () async {
        isRunning.value = true;
        try {
          await onTapped();
        } finally {
          isRunning.value = false;
        }
      },
      state: isRunning.value ? ButtonState.disabled : ButtonState.active,
      label: label,
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

/// Names a run of rows within a card, below the caption naming the card.
class _RowGroupHeader extends StatelessWidget {
  const _RowGroupHeader(this.title);

  final String title;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
      child: Text(
        title,
        style: typeScale.body.xs.style(
          weight: Weight.emphasized,
          color: palette.text.tertiary,
        ),
      ),
    );
  }
}

class _GroupDataCard extends StatelessWidget {
  const _GroupDataCard({required this.data});

  final GroupDataDebugInfo data;

  @override
  Widget build(BuildContext context) {
    return DeveloperCard(
      caption: 'Group Data',
      children: [
        DeveloperInfoRow(label: 'Legacy Title', value: data.legacyTitle ?? '—'),

        DeveloperInfoRow(
          label: 'Legacy Picture',
          value: data.legacyPicture ? 'yes' : 'no',
        ),

        if (data.encryptedTitle case final title?) ...[
          const _RowGroupHeader('Encrypted Title'),
          DeveloperInfoRow(
            label: 'Ciphertext',
            value: title.ciphertext,
            monospace: true,
          ),
          DeveloperInfoRow(label: 'Nonce', value: title.nonce, monospace: true),
          DeveloperInfoRow(label: 'AAD', value: title.aad, monospace: true),
        ] else
          const DeveloperInfoRow(label: 'Encrypted Title', value: '—'),

        if (data.externalGroupProfile case final profile?) ...[
          const _RowGroupHeader('External Group Profile'),
          DeveloperInfoRow(
            label: 'Object ID',
            value: profile.objectId,
            monospace: true,
          ),
          DeveloperInfoRow(label: 'Size', value: profile.size.toString()),
          DeveloperInfoRow(label: 'Enc Alg', value: profile.encAlg ?? '—'),
          DeveloperInfoRow(
            label: 'Nonce',
            value: profile.nonce,
            monospace: true,
          ),
          DeveloperInfoRow(label: 'AAD', value: profile.aad, monospace: true),
          DeveloperInfoRow(label: 'Hash Alg', value: profile.hashAlg),
          DeveloperInfoRow(
            label: 'Content Hash',
            value: profile.contentHash,
            monospace: true,
          ),
        ] else
          const DeveloperInfoRow(label: 'External Group Profile', value: '—'),
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
    final palette = SemanticPalette.of(context);

    return DeveloperCard(
      fill: isOwn ? palette.backgroundBase.quaternary : null,
      children: [
        _MemberHeader(leafIndex: leafIndex, isOwn: isOwn),
        DeveloperInfoRow(label: 'User ID', value: caps.userId, monospace: true),
        DeveloperInfoRow(label: 'Display Name', value: caps.displayName),
        _ChipListRow(label: 'Versions', values: caps.versions),
        _ChipListRow(label: 'Ciphersuites', values: caps.ciphersuites),
        _ChipListRow(label: 'Extensions', values: caps.extensions),
        _ChipListRow(label: 'Proposals', values: caps.proposals),
        _ChipListRow(
          label: 'App Components',
          values: caps.appData?.components ?? [],
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
            if (caps.appData?.airComponent?.features.pqGroups == true)
              'pq_groups',
          ],
        ),
      ],
    );
  }
}

class _MemberHeader extends StatelessWidget {
  const _MemberHeader({required this.leafIndex, required this.isOwn});

  final int leafIndex;
  final bool isOwn;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
      child: Row(
        children: [
          Text(
            'Leaf $leafIndex',
            style: typeScale.body.s.style(
              weight: Weight.emphasized,
              color: palette.text.primary,
            ),
          ),
          if (isOwn) ...[
            const SizedBox(width: S.s8),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(
                color: palette.accentBrand.primary.withValues(alpha: Alpha.a15),
                borderRadius: BorderRadius.circular(CornerRadius.px4),
              ),
              child: Text(
                'self',
                style: typeScale.body.xs.style(
                  color: palette.accentBrand.primary,
                  weight: Weight.emphasized,
                ),
              ),
            ),
          ],
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
    if (values.isEmpty) {
      return DeveloperInfoRow(label: label, value: '—');
    }

    return DeveloperInfoRow(
      label: label,
      value: values.join(', '),
      content: Wrap(
        spacing: S.s8,
        runSpacing: S.s8,
        children: [for (final value in values) _Chip(value)],
      ),
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip(this.label);

  final String label;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: palette.fill.primary,
        borderRadius: BorderRadius.circular(CornerRadius.px4),
      ),
      child: Text(
        label,
        style: typeScale.body.xs
            .style(color: palette.text.secondary)
            .withSystemMonospace(),
      ),
    );
  }
}
