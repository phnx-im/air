// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/message_list/timestamp.dart';
import 'package:flutter/widgets.dart';

/// How long the pointer rests on a message before its time appears. Long enough
/// that skimming a conversation raises nothing.
const Duration _wait = Duration(milliseconds: 600);

/// Clearance from the buttons. Wider than the gap between the buttons
/// themselves, so the time reads as a label beside them rather than a third
/// control among them.
const double _gap = S.s16;

/// A message's time -- the same stamp the meta row carries -- shown at the end
/// of the react and reply buttons once the pointer has rested on the row.
///
/// This is how the time stays reachable on the rows that carry no stamp of
/// their own. It rides in the buttons' own row, so it scrolls and clips with
/// the conversation like everything else, and it takes no width from the
/// bubble: the row's width is already spoken for, so the label hangs off the
/// end of the buttons into the gutter rather than laying itself out there.
///
/// A row that already shows a stamp has nothing to add, so it passes
/// `enabled: false` and stays quiet.
class MessageHoverTime extends StatefulWidget {
  const MessageHoverTime({
    super.key,
    required this.timestamp,
    required this.hovered,
    required this.isSelf,
    required this.enabled,
  });

  final DateTime timestamp;

  /// Whether the pointer is on the row. The wait starts when it arrives and is
  /// dropped when it leaves.
  final bool hovered;

  /// Own message. Its buttons sit on the row's left, so the time hangs further
  /// left still.
  final bool isSelf;

  final bool enabled;

  @override
  State<MessageHoverTime> createState() => _MessageHoverTimeState();
}

class _MessageHoverTimeState extends State<MessageHoverTime> {
  Timer? _timer;
  bool _shown = false;

  @override
  void initState() {
    super.initState();
    _sync();
  }

  @override
  void didUpdateWidget(covariant MessageHoverTime oldWidget) {
    super.didUpdateWidget(oldWidget);
    _sync();
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  /// Brings the wait in line with the row's state rather than with a change in
  /// it. A row can turn revealable under a pointer that is already resting on
  /// it -- a newer message arrives and takes the stamp off this one -- and the
  /// wait has to start there too, not only where the pointer arrives.
  void _sync() {
    if (widget.hovered && widget.enabled) {
      if (_shown || _timer != null) return;
      _timer = Timer(_wait, _reveal);
      return;
    }
    _timer?.cancel();
    _timer = null;
    if (_shown) setState(() => _shown = false);
  }

  void _reveal() {
    _timer = null;
    if (mounted) setState(() => _shown = true);
  }

  @override
  Widget build(BuildContext context) {
    if (!_shown) return const SizedBox.shrink();

    // A point rather than a box, so the bubble beside it keeps every pixel it
    // had: what the label needs, it borrows from the gutter it paints into. The
    // point is centered in the buttons' row, and the label is centered on the
    // point, which is what lines the two up.
    return SizedBox.shrink(
      child: OverflowBox(
        maxWidth: double.infinity,
        maxHeight: double.infinity,
        alignment: widget.isSelf ? Alignment.centerRight : Alignment.centerLeft,
        child: Padding(
          padding: EdgeInsets.only(
            left: widget.isSelf ? S.s0 : _gap,
            right: widget.isSelf ? _gap : S.s0,
          ),
          child: _FadeIn(
            child: MessageTimestamp(
              timestamp: widget.timestamp,
              builder: (context, label) => Text(
                label,
                softWrap: false,
                style: typeScale.body.mini.style(
                  color: SemanticPalette.of(context).text.tertiary,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Brings the label up where it is revealed, rather than having it appear
/// mid-conversation at full strength.
class _FadeIn extends StatelessWidget {
  const _FadeIn({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => TweenAnimationBuilder<double>(
    tween: Tween(begin: Alpha.a0, end: Alpha.a100),
    duration: Effect.duration(MotionPreset.regular),
    curve: Effect.easeOutQuart,
    builder: (context, opacity, child) =>
        Opacity(opacity: opacity, child: child),
    child: child,
  );
}
