// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details_cubit.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/font_size.dart';
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

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          loc.muteDurationSheet_title,
          style: Theme.of(context).textTheme.titleLarge!.copyWith(
            fontWeight: FontWeight.bold,
            color: colors.text.primary,
          ),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Spacing.px16),
        _DurationOption(
          label: loc.muteDurationSheet_1hour,
          mutedUntil: () =>
              DateTime.now().toUtc().add(const Duration(hours: 1)),
        ),
        _DurationOption(
          label: loc.muteDurationSheet_8hours,
          mutedUntil: () =>
              DateTime.now().toUtc().add(const Duration(hours: 8)),
        ),
        _DurationOption(
          label: loc.muteDurationSheet_untilTomorrow,
          mutedUntil: _untilTomorrow,
        ),
        _DurationOption(
          label: loc.muteDurationSheet_untilNextMonday,
          mutedUntil: _untilNextMonday,
        ),
        _DurationOption(
          label: loc.muteDurationSheet_always,
          mutedUntil: () => null,
        ),
        const SizedBox(height: Spacing.px8),
      ],
    );
  }

  static DateTime _untilTomorrow() {
    final now = DateTime.now();
    return DateTime(now.year, now.month, now.day + 1).toUtc();
  }

  static DateTime _untilNextMonday() {
    final now = DateTime.now();
    final daysUntilMonday = (DateTime.monday - now.weekday + 7) % 7;
    final days = daysUntilMonday == 0 ? 7 : daysUntilMonday;
    return DateTime(now.year, now.month, now.day + days).toUtc();
  }
}

class _DurationOption extends StatelessWidget {
  const _DurationOption({required this.label, required this.mutedUntil});

  final String label;
  // Returns null for "always muted"
  final DateTime? Function() mutedUntil;

  @override
  Widget build(BuildContext context) {
    return TextButton(
      onPressed: () {
        Navigator.of(context).pop();
        context.read<ChatDetailsCubit>().muteChat(mutedUntil: mutedUntil());
      },
      style: TextButton.styleFrom(
        alignment: Alignment.centerLeft,
        padding: const EdgeInsets.symmetric(vertical: Spacing.px12),
        foregroundColor: CustomColorScheme.of(context).text.primary,
      ),
      child: Text(
        label,
        style: TextStyle(fontSize: LabelFontSize.base.size),
      ),
    );
  }
}
