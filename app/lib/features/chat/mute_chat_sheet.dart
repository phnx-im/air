// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

Future<void> showMuteChatSheet(BuildContext context) {
  final cubit = context.read<ChatDetailsCubit>();

  return showAdaptiveModal(
    context: context,
    builder: (modalContext) =>
        BlocProvider.value(value: cubit, child: const _MuteDurationContent()),
  );
}

class _MuteDurationContent extends StatelessWidget {
  const _MuteDurationContent();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final theme = Theme.of(context);

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        Text(
          loc.muteDurationSheet_title,
          style: theme.textTheme.titleLarge?.copyWith(
            fontWeight: .bold,
            color: palette.text.primary,
          ),
          textAlign: .center,
        ),
        const SizedBox(height: S.s16),
        Text(
          loc.muteDurationSheet_body,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: palette.text.secondary,
            height: 1.4,
          ),
          textAlign: .center,
        ),
        const SizedBox(height: S.s24),
        _DurationOption(
          label: loc.muteDurationSheet_1hour,
          mutedUntil: UiChatMutedExtension.inOneHour,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_8hours,
          mutedUntil: UiChatMutedExtension.inEightHours,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_untilTomorrow,
          mutedUntil: UiChatMutedExtension.untilTomorrow,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_untilNextMonday,
          mutedUntil: UiChatMutedExtension.untilNextMonday,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_always,
          mutedUntil: () => const UiChatMuted.forever(),
        ),
        const SizedBox(height: S.s8),
        if (DeviceType.isDesktop) ...[
          const SizedBox(height: S.s8),
          Button(
            type: ButtonType.secondary,
            onPressed: () => Navigator.of(context).pop(),
            label: MaterialLocalizations.of(context).cancelButtonLabel,
          ),
        ],
      ],
    );
  }
}

class _DurationOption extends StatelessWidget {
  const _DurationOption({required this.label, required this.mutedUntil});

  final String label;
  final UiChatMuted? Function() mutedUntil;

  @override
  Widget build(BuildContext context) {
    return Button(
      type: ButtonType.secondary,
      onPressed: () {
        Navigator.of(context).pop();
        context.read<ChatDetailsCubit>().muteChat(mutedUntil: mutedUntil());
      },
      label: label,
    );
  }
}
