// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

Future<void> showMuteChatSheet(BuildContext context) {
  return showBottomSheetModal(
    context: context,
    builder: (sheetContext) => BlocProvider.value(
      value: context.read<ChatDetailsCubit>(),
      child: const _MuteChatSheetContent(),
    ),
  );
}

class _MuteChatSheetContent extends StatelessWidget {
  const _MuteChatSheetContent();

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
          mutedUntil: _inOneHour,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_8hours,
          mutedUntil: _inEightHours,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_untilTomorrow,
          mutedUntil: _untilTomorrow,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_untilNextMonday,
          mutedUntil: _untilNextMonday,
        ),
        const SizedBox(height: Spacing.px8),
        _DurationOption(
          label: loc.muteDurationSheet_always,
          mutedUntil: () => const UiChatMuted.forever(),
        ),
        const SizedBox(height: Spacing.px8),
      ],
    );
  }

  static UiChatMuted? _inOneHour() {
    return UiChatMuted.until(DateTime.now().add(const Duration(hours: 1)));
  }

  static UiChatMuted? _inEightHours() {
    return UiChatMuted.until(DateTime.now().add(const Duration(hours: 8)));
  }

  /// until tomorrow, midnight
  static UiChatMuted _untilTomorrow() {
    final now = DateTime.now();
    return UiChatMuted.until(
      DateTime(now.year, now.month, now.day + 1).toUtc(),
    );
  }

  // until next monday, midnight
  static UiChatMuted _untilNextMonday() {
    final now = DateTime.now();
    final daysUntilMonday = (DateTime.monday - now.weekday + 7) % 7;
    final days = daysUntilMonday == 0 ? 7 : daysUntilMonday;
    return UiChatMuted.until(
      DateTime(now.year, now.month, now.day + days).toUtc(),
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
      type: AppButtonType.secondary,
      onPressed: () {
        Navigator.of(context).pop();
        context.read<ChatDetailsCubit>().muteChat(mutedUntil: mutedUntil());
      },
      label: label,
    );
  }
}
