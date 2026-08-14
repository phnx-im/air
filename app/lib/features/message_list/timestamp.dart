// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/util/time/app_clock.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter/material.dart';

/// Keeps a message's stamp current: the elapsed minutes for the first hour, the
/// clock time after that. See [messageStampLabel].
class MessageTimestamp extends StatelessWidget {
  const MessageTimestamp({
    super.key,
    required this.timestamp,
    required this.builder,
  });

  final DateTime timestamp;

  /// Takes the formatted, localized stamp.
  final Widget Function(BuildContext context, String label) builder;

  @override
  Widget build(BuildContext context) => LiveTime(
    format: (context, now) => messageStampLabel(
      timestamp,
      now: now,
      formats: TimeFormats.of(context),
      loc: AppLocalizations.of(context),
    ),
    builder: builder,
  );
}

/// A message's time, on its own. Set like the stamp under a bubble, so an
/// event's time and a message's read as the same label.
class Timestamp extends StatelessWidget {
  const Timestamp(this.timestamp, {super.key});

  final DateTime timestamp;

  @override
  Widget build(BuildContext context) => MessageTimestamp(
    timestamp: timestamp,
    builder: (context, label) => SelectionContainer.disabled(
      child: Text(
        label,
        style: typeScale.body.mini.style(
          color: SemanticPalette.of(context).text.tertiary,
          tight: true,
        ),
      ),
    ),
  );
}
