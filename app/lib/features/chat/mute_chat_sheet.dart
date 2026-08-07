// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';

Future<void> showMuteChatSheet(
  BuildContext context, {
  required void Function({required UiChatMuted? until}) onMute,
}) {
  return showAdaptiveModal(
    context: context,
    builder: (modalContext) => _MuteDurationContent(onMute: onMute),
  );
}

class _MuteDurationContent extends StatelessWidget {
  const _MuteDurationContent({required this.onMute});

  final void Function({required UiChatMuted? until}) onMute;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final theme = Theme.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          loc.muteDurationSheet_title,
          style: theme.textTheme.titleLarge?.copyWith(
            fontWeight: FontWeight.bold,
            color: palette.text.primary,
          ),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: S.s16),
        Text(
          loc.muteDurationSheet_body,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: palette.text.secondary,
            height: 1.4,
          ),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: S.s24),
        _DurationOption(
          label: loc.muteDurationSheet_1hour,
          mutedUntil: UiChatMutedExtension.inOneHour,
          onMute: onMute,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_8hours,
          mutedUntil: UiChatMutedExtension.inEightHours,
          onMute: onMute,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_untilTomorrow,
          mutedUntil: UiChatMutedExtension.untilTomorrow,
          onMute: onMute,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_untilNextMonday,
          mutedUntil: UiChatMutedExtension.untilNextMonday,
          onMute: onMute,
        ),
        const SizedBox(height: S.s8),
        _DurationOption(
          label: loc.muteDurationSheet_always,
          mutedUntil: () => const UiChatMuted.forever(),
          onMute: onMute,
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
  const _DurationOption({
    required this.label,
    required this.mutedUntil,
    required this.onMute,
  });

  final String label;
  final UiChatMuted? Function() mutedUntil;
  final void Function({required UiChatMuted? until}) onMute;

  @override
  Widget build(BuildContext context) {
    return Button(
      type: ButtonType.secondary,
      onPressed: () {
        Navigator.of(context).pop();
        onMute(until: mutedUntil());
      },
      label: label,
    );
  }
}
