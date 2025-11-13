// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';

class Timestamp extends StatefulWidget {
  const Timestamp(this.timestamp, {super.key});

  final DateTime timestamp;

  @override
  State<Timestamp> createState() => TimestampState();
}

class TimestampState extends State<Timestamp> {
  String _displayTimestamp = '';
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _displayTimestamp = _calcTimeString(widget.timestamp);
    _timer = Timer.periodic(const Duration(seconds: 5), (timer) {
      final newDisplayTimestamp = _calcTimeString(widget.timestamp);
      if (newDisplayTimestamp != _displayTimestamp) {
        setState(() {
          _displayTimestamp = _calcTimeString(widget.timestamp);
        });
      }
    });
  }

  @override
  void didUpdateWidget(covariant Timestamp oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.timestamp != widget.timestamp) {
      setState(() {
        _displayTimestamp = _calcTimeString(widget.timestamp);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final localizedTimestamp = _localizedTimeString(_displayTimestamp, context);
    return SelectionContainer.disabled(
      child: Text(
        localizedTimestamp,
        style: TextStyle(
          color: CustomColorScheme.of(context).text.tertiary,
          fontSize: LabelFontSize.small2.size,
        ),
      ),
    );
  }

  String _calcTimeString(DateTime t) {
    // If the elapsed time is less than 60 seconds, show "now"
    if (DateTime.now().difference(t).inSeconds < 60) {
      return "now";
    }
    // If the elapsed time is less than 60 minutes, show the elapsed minutes
    if (DateTime.now().difference(t).inMinutes < 60) {
      return '${DateTime.now().difference(t).inMinutes}m';
    }
    // Otherwise show the time
    return '${t.hour}:${t.minute.toString().padLeft(2, '0')}';
  }

  String _localizedTimeString(String time, BuildContext context) {
    if (time == "now") {
      return AppLocalizations.of(context).timestamp_now;
    } else {
      return time;
    }
  }
}
