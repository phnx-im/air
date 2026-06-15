// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/modal/app_dialog.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/theme/responsive_screen.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

Future<void> showMuteChatSheet(BuildContext context) {
  final cubit = context.read<ChatDetailsCubit>();

  if (ResponsiveScreen.isDesktop(context)) {
    return showDialog(
      context: context,
      builder: (dialogContext) => AppDialog(
        child: BlocProvider.value(
          value: cubit,
          child: const _MuteDurationContent(),
        ),
      ),
    ).then((_) {});
  }

  return showBottomSheetModal(
    context: context,
    builder: (sheetContext) =>
        BlocProvider.value(value: cubit, child: const _MuteDurationContent()),
  );
}

class _MuteDurationContent extends StatelessWidget {
  const _MuteDurationContent();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    final theme = Theme.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          loc.muteDurationSheet_title,
          style: theme.textTheme.titleLarge?.copyWith(
            fontWeight: FontWeight.bold,
            color: colors.text.primary,
          ),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Spacing.px16),
        Text(
          loc.muteDurationSheet_body,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: colors.text.secondary,
            height: 1.4,
          ),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Spacing.px24),
        _DurationOption(
          label: loc.muteDurationSheet_1hour,
          mutedUntil: UiChatMutedExtension.inOneHour,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_8hours,
          mutedUntil: UiChatMutedExtension.inEightHours,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_untilTomorrow,
          mutedUntil: UiChatMutedExtension.untilTomorrow,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_untilNextMonday,
          mutedUntil: UiChatMutedExtension.untilNextMonday,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_always,
          mutedUntil: () => const UiChatMuted.forever(),
        ),
        const SizedBox(height: Spacing.px8),
        if (ResponsiveScreen.isDesktop(context)) ...[
          const SizedBox(height: Spacing.px8),
          AppButton(
            type: AppButtonType.secondary,
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
    return AppButton(
      type: AppButtonType.primary,
      onPressed: () {
        Navigator.of(context).pop();
        context.read<ChatDetailsCubit>().muteChat(mutedUntil: mutedUntil());
      },
      label: label,
    );
  }
}
