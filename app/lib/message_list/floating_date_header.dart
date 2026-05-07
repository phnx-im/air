// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/ui/effects/motion.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'date_divider.dart';

/// Pill that surfaces the date of the topmost visible message while the
/// user scrolls, mirroring [DateDivider]'s label so the swap from inline
/// to floating is invisible.
///
/// Visibility gates on both [scrollActive] (fades out at rest) and
/// [isOldestVisibleHoisted] (hides while no inline divider is being
/// substituted). Self-ticks once a minute so Today/Yesterday rollover
/// keeps pace with the wall clock.
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
  Timer? _timer;

  /// Tracked outside build so the slide direction can be decided without
  /// mutating state during the build phase.
  DateTime? _previousTimestamp;
  bool _newFromBelow = true;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(minutes: 1), (_) {
      if (mounted) setState(() {});
    });
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
    _timer?.cancel();
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
            duration: motionRegular,
            curve: motionEasing,
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

              final loc = AppLocalizations.of(context);
              final locale = Localizations.localeOf(context).toString();
              final label = formatDateLabel(
                timestamp,
                DateTime.now(),
                loc,
                locale,
              );
              return AnimatedSwitcher(
                duration: motionShort,
                switchInCurve: Curves.easeOut,
                switchOutCurve: Curves.easeOut,
                transitionBuilder: (child, animation) =>
                    _slideFadeTransition(child, animation, newFromBelow),
                layoutBuilder: _stackedLayoutBuilder,
                child: DateLabelPill(key: ValueKey(label), label: label),
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
