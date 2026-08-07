// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/message_separator/message_separator.dart';
import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/util/time/app_clock.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter/widgets.dart';

/// Section header showing the local day of the messages below; see
/// [dateDividerLabel] for the label rules. Keeps up with the clock, so the
/// Today/Yesterday rollover happens on its own.
class DateDivider extends StatelessWidget {
  const DateDivider({super.key, required this.date});

  final DateTime date;

  @override
  Widget build(BuildContext context) => LiveTime(
    format: (context, now) => dividerLabel(context, date, now),
    builder: (context, label) => MessageSeparator(label: label),
  );
}

/// The divider's label for [date], as read at [now].
String dividerLabel(BuildContext context, DateTime date, DateTime now) =>
    dateDividerLabel(
      date,
      now: now,
      formats: TimeFormats.of(context),
      loc: AppLocalizations.of(context),
    );
