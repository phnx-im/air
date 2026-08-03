// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

/// How often the clock is read. Fine enough for a stamp to turn over from
/// "Now" to "1m" without a visible lag.
const _tick = Duration(seconds: 5);

/// The one clock every live time label reads, so a screen full of stamps costs
/// a single timer rather than one per row.
///
/// Install it once above the app. A label with no clock above it reads the wall
/// clock as it mounts and then holds still, which is what a widget test wants.
class AppClock extends StatefulWidget {
  const AppClock({super.key, required this.child});

  final Widget child;

  /// The ticking clock, or null where none is installed.
  static ValueListenable<DateTime>? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_AppClockScope>()?.now;

  @override
  State<AppClock> createState() => _AppClockState();
}

class _AppClockState extends State<AppClock> {
  final ValueNotifier<DateTime> _now = ValueNotifier(DateTime.now());
  late final Timer _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(_tick, (_) => _now.value = DateTime.now());
  }

  @override
  void dispose() {
    _timer.cancel();
    _now.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      _AppClockScope(now: _now, child: widget.child);
}

/// Carries the clock without rebuilding on it: dependents take the listenable
/// and subscribe to it themselves, so a tick reaches only the labels whose text
/// it changes.
class _AppClockScope extends InheritedWidget {
  const _AppClockScope({required this.now, required super.child});

  final ValueListenable<DateTime> now;

  @override
  bool updateShouldNotify(_AppClockScope oldWidget) => now != oldWidget.now;
}

/// A label that keeps up with the clock, rebuilding only when its text changes.
///
/// [format] runs against every tick and is expected to be cheap; the label a
/// tick produces is compared with the one on screen, and an unchanged label
/// costs nothing beyond that comparison. This is what lets a stamp reading
/// `14:32` sit still for hours while the one reading `3m` counts up.
class LiveTime extends StatefulWidget {
  const LiveTime({super.key, required this.format, required this.builder});

  /// Writes the label for a given reading of the clock.
  final String Function(BuildContext context, DateTime now) format;

  final Widget Function(BuildContext context, String label) builder;

  @override
  State<LiveTime> createState() => _LiveTimeState();
}

class _LiveTimeState extends State<LiveTime> {
  ValueListenable<DateTime>? _clock;
  late String _label;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final clock = AppClock.maybeOf(context);
    if (clock != _clock) {
      _clock?.removeListener(_refresh);
      clock?.addListener(_refresh);
      _clock = clock;
    }
    // The locale and the platform's patterns are dependencies too, so the label
    // is rewritten here rather than only on a tick.
    _label = _read();
  }

  @override
  void didUpdateWidget(covariant LiveTime oldWidget) {
    super.didUpdateWidget(oldWidget);
    // A build follows, so the label is written straight into the field.
    _label = _read();
  }

  @override
  void dispose() {
    _clock?.removeListener(_refresh);
    super.dispose();
  }

  String _read() => widget.format(context, _clock?.value ?? DateTime.now());

  void _refresh() {
    final next = _read();
    if (next != _label) setState(() => _label = next);
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, _label);
}
