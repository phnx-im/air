// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' show min;

import 'package:air/features/message_list/time_reveal.dart';
import 'package:air/platform/haptics.dart';
import 'package:flutter/material.dart';
import 'package:flutter/physics.dart';

/// Maximum distance (in logical pixels) the bubble can slide.
/// Must be strictly greater than [_triggerThreshold].
const double _maxOffset = 80.0;

/// Fixed pixel threshold at which the reply action triggers.
const double _triggerThreshold = 60.0;

/// Damping factor applied to drag past the trigger point, producing a
/// rubber-band effect.
const double _rubberBandFactor = 0.25;

/// Padding between the reply icon and the bubble's visible left edge.
const double _iconPadding = 12.0;

/// Spring parameters for the snap-back animation.
const double _springStiffness = 400.0;
const double _springDamping = 28.0;

/// Gesture area for swipe-to-reply.
///
/// Place this high in the widget tree to define the hit-test area for the
/// swipe gesture (typically the full message row). A [SwipeToReplyBubble]
/// descendant reads the animation state to render the icon and translation.
///
/// Swiping from left to right beyond [_triggerThreshold] fires [onReply].
/// Swiping the other way pulls out the shared timestamp column, where a
/// [TimeRevealScope] offers one: the first movement decides which of the two the
/// whole gesture is, so neither can be triggered by accident on the way to the
/// other.
class SwipeToReplyScope extends StatefulWidget {
  const SwipeToReplyScope({
    super.key,
    required this.onReply,
    required this.child,
  });

  /// Called once when the swipe crosses the trigger threshold and the
  /// user lifts their finger.
  final VoidCallback onReply;

  final Widget child;

  @override
  State<SwipeToReplyScope> createState() => _SwipeToReplyScopeState();
}

/// What the drag in progress turned out to be.
enum _DragIntent { undecided, reply, revealTime }

class _SwipeToReplyScopeState extends State<SwipeToReplyScope>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  /// Raw accumulated drag distance (before damping).
  double _rawDragOffset = 0.0;

  /// Whether the trigger threshold was already crossed during this drag.
  bool thresholdCrossed = false;

  /// Whether a drag gesture is in progress.
  bool isDragging = false;

  _DragIntent _intent = _DragIntent.undecided;
  TimeRevealController? _timeReveal;

  @override
  void initState() {
    super.initState();
    assert(
      _maxOffset > _triggerThreshold,
      '_maxOffset must exceed _triggerThreshold',
    );
    _controller = AnimationController.unbounded(vsync: this);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _timeReveal = TimeRevealScope.maybeOf(context);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  /// Converts a raw drag distance into a damped offset capped at [_maxOffset].
  double _dampedOffset(double raw) {
    if (raw <= _triggerThreshold) return raw;
    final overshoot = raw - _triggerThreshold;
    return min(_triggerThreshold + overshoot * _rubberBandFactor, _maxOffset);
  }

  void _onDragStart(DragStartDetails details) {
    // Ignore new drags while the spring-back animation is running to
    // prevent double-firing onReply on rapid re-swipes.
    if (_controller.isAnimating) return;
    isDragging = true;
    thresholdCrossed = false;
    _rawDragOffset = 0.0;
    _intent = _DragIntent.undecided;
    _controller.stop();
  }

  void _onDragUpdate(DragUpdateDetails details) {
    if (!isDragging) return;
    final reveal = _timeReveal;
    if (_intent == _DragIntent.undecided && details.delta.dx != 0) {
      _intent = (details.delta.dx < 0 && reveal != null)
          ? _DragIntent.revealTime
          : _DragIntent.reply;
    }
    if (_intent == _DragIntent.revealTime) {
      reveal?.drag(-details.delta.dx);
      return;
    }
    _rawDragOffset = (_rawDragOffset + details.delta.dx).clamp(
      0.0,
      double.infinity,
    );
    _controller.value = _dampedOffset(_rawDragOffset);

    if (_rawDragOffset >= _triggerThreshold) {
      if (!thresholdCrossed) {
        thresholdCrossed = true;
        AppHaptics.gestureTrigger();
      }
    } else {
      thresholdCrossed = false;
    }
  }

  void _onDragEnd(DragEndDetails details) {
    if (!isDragging) return;
    isDragging = false;
    if (_release()) return;
    if (thresholdCrossed) {
      widget.onReply();
    }
    _springBack();
  }

  void _onDragCancel() {
    if (!isDragging) return;
    isDragging = false;
    if (_release()) return;
    _springBack();
  }

  /// Hands a finished reveal drag back to the column, which slides away. True
  /// when the gesture was one, and the reply half has nothing to undo.
  bool _release() {
    if (_intent != _DragIntent.revealTime) return false;
    _intent = _DragIntent.undecided;
    _timeReveal?.release();
    return true;
  }

  void _springBack() {
    final simulation = SpringSimulation(
      const SpringDescription(
        mass: 1,
        stiffness: _springStiffness,
        damping: _springDamping,
      ),
      _controller.value,
      0.0,
      0.0,
    );
    _controller.animateWith(simulation);
  }

  @override
  Widget build(BuildContext context) {
    return _SwipeToReplyInherited(
      controller: _controller,
      state: this,
      child: GestureDetector(
        behavior: HitTestBehavior.translucent,
        onHorizontalDragStart: _onDragStart,
        onHorizontalDragUpdate: _onDragUpdate,
        onHorizontalDragEnd: _onDragEnd,
        onHorizontalDragCancel: _onDragCancel,
        child: widget.child,
      ),
    );
  }
}

/// Visual animation for swipe-to-reply.
///
/// Must be a descendant of [SwipeToReplyScope]. Renders the reply [icon]
/// sliding in from the left and translates the [child] (the message bubble)
/// to the right in sync with the drag.
class SwipeToReplyBubble extends StatelessWidget {
  const SwipeToReplyBubble({
    super.key,
    required this.icon,
    required this.child,
  });

  /// Icon displayed behind the bubble while swiping.
  final Widget icon;

  /// The message bubble.
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final inherited = _SwipeToReplyInherited.of(context);
    final controller = inherited.controller;
    final state = inherited.state;

    return AnimatedBuilder(
      animation: controller,
      builder: (context, child) {
        final offset = controller.value;
        final iconProgress = (offset / _triggerThreshold).clamp(0.0, 1.0);
        final iconScale = (state.isDragging && state.thresholdCrossed)
            ? 1.0 +
                  0.2 *
                      (1.0 -
                          ((offset - _triggerThreshold).abs() /
                                  (_maxOffset - _triggerThreshold))
                              .clamp(0.0, 1.0))
            : iconProgress;

        return Stack(
          clipBehavior: Clip.none,
          children: [
            Positioned(
              left: 0,
              top: 0,
              bottom: 0,
              width: offset,
              child: Opacity(
                opacity: iconProgress,
                child: Transform.scale(
                  scale: iconScale,
                  child: Align(
                    alignment: Alignment.centerRight,
                    child: Padding(
                      padding: const EdgeInsets.only(right: _iconPadding),
                      child: icon,
                    ),
                  ),
                ),
              ),
            ),
            Transform.translate(offset: Offset(offset, 0), child: child),
          ],
        );
      },
      child: child,
    );
  }
}

class _SwipeToReplyInherited extends InheritedWidget {
  const _SwipeToReplyInherited({
    required this.controller,
    required this.state,
    required super.child,
  });

  final AnimationController controller;
  final _SwipeToReplyScopeState state;

  static _SwipeToReplyInherited of(BuildContext context) {
    final result = context
        .dependOnInheritedWidgetOfExactType<_SwipeToReplyInherited>();
    assert(result != null, 'No SwipeToReplyScope found in ancestors');
    return result!;
  }

  @override
  bool updateShouldNotify(_SwipeToReplyInherited oldWidget) =>
      controller != oldWidget.controller;
}
