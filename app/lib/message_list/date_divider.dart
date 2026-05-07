// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

/// Section header showing the local day of the messages below; see
/// [formatDateLabel] for the label rules. Self-ticks once a minute so
/// Today/Yesterday rollover keeps pace with the wall clock.
class DateDivider extends StatefulWidget {
  const DateDivider({super.key, required this.date});

  final DateTime date;

  @override
  State<DateDivider> createState() => _DateDividerState();
}

class _DateDividerState extends State<DateDivider> {
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(minutes: 1), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final locale = Localizations.localeOf(context).toString();
    final label = formatDateLabel(widget.date, DateTime.now(), loc, locale);

    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.m,
        vertical: Spacings.l,
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [DateLabelPill(label: label)],
      ),
    );
  }
}

class DateLabelPill extends StatelessWidget {
  const DateLabelPill({super.key, required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: ShapeDecoration(
        color: CustomColorScheme.of(context).backgroundBase.secondary,
        shape: const StadiumBorder(),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.s,
          vertical: Spacings.xxxs,
        ),
        child: Text(
          label,
          style: TextTheme.of(context).bodySmall?.copyWith(
            color: CustomColorScheme.of(context).text.secondary,
          ),
        ),
      ),
    );
  }
}

/// Picks a date-pill label for [date] in local time, relative to [now].
///
/// Days are calendar days, not 24-hour windows. First match wins:
/// - same day: "Today"
/// - one day back: "Yesterday"
/// - two to six days back: localized weekday name
/// - same year, older: abbreviated weekday + month + day (no year)
/// - earlier years: localized medium date with year
String formatDateLabel(
  DateTime date,
  DateTime now,
  AppLocalizations loc,
  String locale,
) {
  final local = date.toLocal();
  final messageDay = DateTime(local.year, local.month, local.day);
  final today = DateTime(now.year, now.month, now.day);
  final daysDiff = today.difference(messageDay).inDays;

  if (daysDiff == 0) return loc.date_today;
  if (daysDiff == 1) return loc.date_yesterday;
  if (daysDiff > 1 && daysDiff < 7) {
    return DateFormat.EEEE(locale).format(local);
  }
  if (today.year == messageDay.year) {
    return DateFormat.MMMEd(locale).format(local);
  }
  return DateFormat.yMMMd(locale).format(local);
}
