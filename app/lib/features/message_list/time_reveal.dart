// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' show max;

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/message_list/timestamp.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter/widgets.dart';

/// Gap between the resting column and the trailing edge of the list.
const double _restingInset = S.s16;

/// Gap a row keeps between its own trailing edge and the label.
const double _labelGap = S.s16;

/// Clocks the column is measured against. Midnight and late evening cover the
/// widest a locale prints: two wide digits either way, and the longer of the
/// two meridiems where the pattern carries one.
///
/// One width serves the whole list, so it has to hold the widest label any row
/// can show rather than the one the column happens to pass. The stamp is not
/// always a clock, so the relative forms are measured alongside these, see
/// [_measureColumn].
final List<DateTime> _widestClocks = [
  DateTime(2026, 1, 1, 0, 58),
  DateTime(2026, 1, 1, 22, 58),
];

/// The oldest a message can be and still read in minutes, which is the widest
/// that form gets.
const int _widestMinutes = 59;

/// Drives the timestamp column every row in a list shares, so one drag reveals
/// the whole conversation's times rather than a single row's.
///
/// Held by the list and read by the rows: the gesture lives in
/// [SwipeToReplyScope], which hands leftward drags here and keeps rightward
/// ones for the reply.
class TimeRevealController {
  TimeRevealController(this._animation, this.width);

  final AnimationController _animation;

  /// How far the label travels on its way in: the resting inset plus the widest
  /// label the locale prints. What a row has to give up for it is the row's own
  /// business, see [TimeRevealRow].
  final double width;

  /// How far the column is out, from 0 (away) to 1 (fully revealed).
  Animation<double> get progress => _animation;

  /// Pulls the column out by [dx] logical pixels of leftward drag.
  void drag(double dx) {
    _animation.value = (_animation.value + dx / width).clamp(0.0, 1.0);
  }

  /// Lets go. The column always returns: this is a peek, not a state the list
  /// can be left in.
  void release() {
    // The whole return however far the column came out. Left to scale itself,
    // the return takes only the fraction of the duration that matches the
    // distance, and a half-open column blinks away rather than travelling.
    _animation.animateTo(
      0,
      duration: Effect.duration(MotionPreset.regular),
      curve: Effect.easeOutQuart,
    );
  }
}

/// Owns the reveal for the list below it. Install it on phones, where the drag
/// is the way to reach a time the rows do not show.
class TimeRevealScope extends StatefulWidget {
  const TimeRevealScope({super.key, required this.child});

  final Widget child;

  /// The list's reveal, or null where nothing is revealable -- on a desktop
  /// list, where the pointer takes this job.
  static TimeRevealController? maybeOf(BuildContext context) => context
      .dependOnInheritedWidgetOfExactType<_TimeRevealScope>()
      ?.controller;

  @override
  State<TimeRevealScope> createState() => _TimeRevealScopeState();
}

class _TimeRevealScopeState extends State<TimeRevealScope>
    with SingleTickerProviderStateMixin {
  // Unbounded in time: the drag sets the value outright, and the return home
  // carries its own duration.
  late final AnimationController _animation = AnimationController(vsync: this);
  late TimeRevealController _controller;
  double? _width;

  /// The column is only as wide as the locale and the text scale make it, so it
  /// is measured here rather than fixed: a handed-down width either crops a
  /// long label or leaves a hole where a short one sits.
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final width = _measureColumn(context);
    if (width != _width) {
      _width = width;
      // A new controller rather than a mutated one: the rows read the width
      // where they build, and the inherited widget only tells them to build
      // again when the controller itself changes.
      _controller = TimeRevealController(_animation, width);
    }
  }

  @override
  void dispose() {
    _animation.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      _TimeRevealScope(controller: _controller, child: widget.child);
}

class _TimeRevealScope extends InheritedWidget {
  const _TimeRevealScope({required this.controller, required super.child});

  final TimeRevealController controller;

  @override
  bool updateShouldNotify(_TimeRevealScope oldWidget) =>
      controller != oldWidget.controller;
}

/// The label's style. The column is measured against it, so the two come from
/// one place.
///
/// Spells out the tracking the token leaves open: the measuring happens at the
/// list and the drawing down in a row, and the ambient text style is not the
/// same in both places.
TextStyle _labelStyle(BuildContext context) => typeScale.body.mini
    .style(color: SemanticPalette.of(context).text.tertiary)
    .copyWith(letterSpacing: typeScale.body.mini.letterSpacing);

/// The widest the column has to be for the labels this locale can print.
///
/// Every form [messageStampLabel] can produce is measured, not just the clock:
/// a recent message reads "Now" or in minutes, and in some locales those run
/// wider than any clock does.
double _measureColumn(BuildContext context) {
  // Resolve the style the way [Text] does: a bare [TextPainter] falls back to
  // the engine's default font family rather than the one the label renders in.
  final style = DefaultTextStyle.of(context).style.merge(_labelStyle(context));
  final formats = TimeFormats.of(context);
  final loc = AppLocalizations.of(context);
  final direction = Directionality.of(context);
  final scaler = MediaQuery.textScalerOf(context);
  var label = 0.0;
  for (final candidate in [
    for (final at in _widestClocks) formats.clock(at),
    loc.timestamp_now,
    loc.timestamp_minutesAgo(_widestMinutes),
  ]) {
    final painter = TextPainter(
      text: TextSpan(text: candidate, style: style),
      textDirection: direction,
      textScaler: scaler,
    )..layout();
    label = max(label, painter.width);
    painter.dispose();
  }
  return _restingInset + label;
}

/// Rides a message's time in from the trailing edge of the list as the shared
/// reveal rises.
///
/// Wraps a whole row, gutter and all: the column belongs to the list's edge, so
/// a label lands in the same place whoever sent the message. A row steps aside
/// only as far as the label needs -- a bubble that already stops short of the
/// column holds still, and one that runs the width of the row gives way.
class TimeRevealRow extends StatefulWidget {
  const TimeRevealRow({
    super.key,
    required this.timestamp,
    required this.bubbleKey,
    required this.child,
  });

  final DateTime timestamp;

  /// Keys the bubble the label lines up with. The row can be taller and wider
  /// than its bubble -- a sender name above it, a stamp below, a gutter beside
  /// it -- and it is the bubble the label has to clear, not the row.
  final GlobalKey bubbleKey;

  final Widget child;

  @override
  State<TimeRevealRow> createState() => _TimeRevealRowState();
}

class _TimeRevealRowState extends State<TimeRevealRow> {
  /// Keys the row inside the reveal's own transform, so what we measure off it
  /// is where the row rests rather than where the reveal has moved it to.
  final GlobalKey _rowKey = GlobalKey();

  @override
  Widget build(BuildContext context) {
    final reveal = TimeRevealScope.maybeOf(context);
    if (reveal == null) return widget.child;

    // The tree holds its shape whether the column is out or away, and the
    // reveal only ever moves what is already there. The row carries the
    // recognizer the drag runs on, and rebuilding it into a different shape
    // partway through takes the drag down with it.
    return AnimatedBuilder(
      animation: reveal.progress,
      child: KeyedSubtree(key: _rowKey, child: widget.child),
      builder: (context, row) {
        final t = reveal.progress.value;
        final (:shift, :alignment) = _geometry(reveal.width);
        return Stack(
          clipBehavior: .none,
          children: [
            Transform.translate(offset: Offset(-t * shift, 0), child: row),
            Positioned(
              right: _restingInset,
              top: 0,
              bottom: 0,
              child: Transform.translate(
                // Parked just past the trailing edge, so a drag pulls it in
                // from off screen rather than out of the row.
                offset: Offset((1 - t) * reveal.width, 0),
                // The label is built only once the column is on its way in:
                // every row in the list carries one.
                child: t == 0
                    ? const SizedBox.shrink()
                    : Opacity(
                        opacity: t,
                        child: Align(alignment: alignment, child: _label()),
                      ),
              ),
            ),
          ],
        );
      },
    );
  }

  /// The same stamp the meta row and the hover label carry, so one message
  /// reads the same however the reader reaches its time.
  Widget _label() => MessageTimestamp(
    timestamp: widget.timestamp,
    builder: (context, label) =>
        Text(label, softWrap: false, style: _labelStyle(context)),
  );

  /// What the row's own layout says about the label: how far the row has to
  /// step aside for a column [width] wide, and where down the row the label
  /// sits. Both come off the last frame's layout, which holds still while the
  /// drag runs.
  ({double shift, Alignment alignment}) _geometry(double width) {
    // Nothing laid out to measure yet, and nothing on screen either: the column
    // is away until a drag brings it out.
    const away = (shift: 0.0, alignment: Alignment.center);
    // Our own row first. Until it is mounted the bubble's key still answers for
    // the row this one replaces, and an element on its way out has no box to
    // ask about.
    final row = _rowKey.currentContext?.findRenderObject() as RenderBox?;
    if (row == null || !row.hasSize || row.size.height == 0) return away;
    final bubble =
        widget.bubbleKey.currentContext?.findRenderObject() as RenderBox?;
    if (bubble == null || !bubble.hasSize || !bubble.attached) return away;
    final corner = bubble.localToGlobal(
      Offset(bubble.size.width, bubble.size.height / 2),
      ancestor: row,
    );
    return (
      // What the bubble leaves free at the trailing edge, against what the
      // label needs there.
      shift: max(0, width + _labelGap - (row.size.width - corner.dx)),
      // Alignment's vertical axis runs -1 (top) to 1 (bottom).
      alignment: Alignment(0, (corner.dy / row.size.height) * 2 - 1),
    );
  }
}
