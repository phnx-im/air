// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_separator/message_separator.dart';
import 'package:air/util/time/app_clock.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'package:air/features/message_list/date_divider.dart';

/// Pill that surfaces the date of the topmost visible message while the
/// user scrolls, mirroring [DateDivider]'s label so the swap from inline
/// to floating is invisible.
///
/// Visibility gates on both [scrollActive] (fades out at rest) and
/// [isOldestVisibleHoisted] (hides while no inline divider is being
/// substituted).
class FloatingDateHeader extends StatefulWidget {
  const FloatingDateHeader({
    super.key,
    required this.oldestVisibleId,
    required this.isOldestVisibleHoisted,
    required this.resolveTimestamp,
    required this.scrollActive,
  });

  final ValueListenable<Object?> oldestVisibleId;
  final ValueListenable<bool> isOldestVisibleHoisted;

  /// Resolves an id to its message timestamp, or null if not loaded.
  final DateTime? Function(Object id) resolveTimestamp;
  final ValueListenable<bool> scrollActive;

  @override
  State<FloatingDateHeader> createState() => _FloatingDateHeaderState();
}

class _FloatingDateHeaderState extends State<FloatingDateHeader> {
  /// Tracked outside build so the slide direction can be decided without
  /// mutating state during the build phase.
  DateTime? _previousTimestamp;
  bool _newFromBelow = true;

  @override
  void initState() {
    super.initState();
    widget.oldestVisibleId.addListener(_onOldestVisibleChanged);
    // Seed _previousTimestamp so the first real change has a baseline.
    _onOldestVisibleChanged();
  }

  @override
  void didUpdateWidget(covariant FloatingDateHeader oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.oldestVisibleId != widget.oldestVisibleId) {
      oldWidget.oldestVisibleId.removeListener(_onOldestVisibleChanged);
      widget.oldestVisibleId.addListener(_onOldestVisibleChanged);
      _onOldestVisibleChanged();
    }
  }

  @override
  void dispose() {
    widget.oldestVisibleId.removeListener(_onOldestVisibleChanged);
    super.dispose();
  }

  void _onOldestVisibleChanged() {
    final id = widget.oldestVisibleId.value;
    if (id == null) return;
    final timestamp = widget.resolveTimestamp(id);
    if (timestamp == null) return;
    final previous = _previousTimestamp;
    if (previous != null) {
      if (timestamp.isAfter(previous)) {
        _newFromBelow = true;
      } else if (timestamp.isBefore(previous)) {
        _newFromBelow = false;
      }
    }
    _previousTimestamp = timestamp;
  }

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      child: AnimatedBuilder(
        animation: widget.scrollActive,
        builder: (context, child) {
          return AnimatedOpacity(
            opacity: widget.scrollActive.value ? 1.0 : 0.0,
            duration: Effect.duration(MotionPreset.regular),
            curve: Effect.easeOutQuart,
            child: child,
          );
        },
        child: ValueListenableBuilder<bool>(
          valueListenable: widget.isOldestVisibleHoisted,
          builder: (context, hoisted, child) {
            return Visibility(
              visible: hoisted,
              maintainState: true,
              maintainAnimation: true,
              child: child!,
            );
          },
          child: ValueListenableBuilder<Object?>(
            valueListenable: widget.oldestVisibleId,
            builder: (context, id, _) {
              final timestamp = id == null ? null : widget.resolveTimestamp(id);
              if (timestamp == null) {
                return const SizedBox.shrink();
              }
              final newFromBelow = _newFromBelow;

              return LiveTime(
                format: (context, now) => dividerLabel(context, timestamp, now),
                builder: (context, label) => AnimatedSwitcher(
                  duration: Effect.duration(MotionPreset.short),
                  switchInCurve: Curves.easeOut,
                  switchOutCurve: Curves.easeOut,
                  transitionBuilder: (child, animation) =>
                      _slideFadeTransition(child, animation, newFromBelow),
                  layoutBuilder: _stackedLayoutBuilder,
                  child: MessageSeparatorPill(
                    key: ValueKey(label),
                    label: label,
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );
  }
}

/// Slides incoming and outgoing pills the same direction. The animation
/// direction depens on the scroll direction.
Widget _slideFadeTransition(
  Widget child,
  Animation<double> animation,
  bool newFromBelow,
) {
  const distance = 16.0;
  return AnimatedBuilder(
    animation: animation,
    builder: (context, _) {
      final t = animation.value;
      final isReverse = animation.status == AnimationStatus.reverse;
      final double dy;
      if (newFromBelow) {
        dy = isReverse ? -(1 - t) * distance : (1 - t) * distance;
      } else {
        dy = isReverse ? (1 - t) * distance : -(1 - t) * distance;
      }
      return Transform.translate(
        offset: Offset(0, dy),
        child: Opacity(opacity: t, child: child),
      );
    },
  );
}

Widget _stackedLayoutBuilder(
  Widget? currentChild,
  List<Widget> previousChildren,
) {
  return Stack(
    alignment: Alignment.center,
    children: [...previousChildren, ?currentChild],
  );
}
