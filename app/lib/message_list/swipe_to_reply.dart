// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' show min;

import 'package:flutter/material.dart';
import 'package:flutter/physics.dart';
import 'package:flutter/services.dart';

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

/// A swipe-to-reply wrapper for message bubbles.
///
/// Swiping the child from left to right beyond [_triggerThreshold] fires
/// [onReply]. The bubble is capped at [_maxOffset], resists further drag
/// with a rubber-band curve, and springs back on release.
class SwipeToReply extends StatefulWidget {
  const SwipeToReply({
    super.key,
    required this.onReply,
    required this.icon,
    required this.child,
  });

  /// Called once when the swipe crosses the trigger threshold and the
  /// user lifts their finger.
  final VoidCallback onReply;

  /// Icon displayed behind the bubble while swiping.
  final Widget icon;

  /// The message bubble.
  final Widget child;

  @override
  State<SwipeToReply> createState() => _SwipeToReplyState();
}

class _SwipeToReplyState extends State<SwipeToReply>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  /// Raw accumulated drag distance (before damping).
  double _rawDragOffset = 0.0;

  /// Whether the trigger threshold was already crossed during this drag.
  bool _thresholdCrossed = false;

  /// Whether a drag gesture is in progress.
  bool _isDragging = false;

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
    _isDragging = true;
    _thresholdCrossed = false;
    _rawDragOffset = 0.0;
    _controller.stop();
  }

  void _onDragUpdate(DragUpdateDetails details) {
    if (!_isDragging) return;
    _rawDragOffset = (_rawDragOffset + details.delta.dx).clamp(
      0.0,
      double.infinity,
    );
    _controller.value = _dampedOffset(_rawDragOffset);

    if (_rawDragOffset >= _triggerThreshold) {
      if (!_thresholdCrossed) {
        _thresholdCrossed = true;
        HapticFeedback.mediumImpact();
      }
    } else {
      _thresholdCrossed = false;
    }
  }

  void _onDragEnd(DragEndDetails details) {
    if (!_isDragging) return;
    _isDragging = false;
    if (_thresholdCrossed) {
      widget.onReply();
    }
    _springBack();
  }

  void _onDragCancel() {
    if (!_isDragging) return;
    _isDragging = false;
    _springBack();
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
    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onHorizontalDragStart: _onDragStart,
      onHorizontalDragUpdate: _onDragUpdate,
      onHorizontalDragEnd: _onDragEnd,
      onHorizontalDragCancel: _onDragCancel,
      child: AnimatedBuilder(
        animation: _controller,
        builder: (context, child) {
          final offset = _controller.value;
          // Icon progress: 0 → 1 over the first triggerThreshold pixels.
          final iconProgress = (offset / _triggerThreshold).clamp(0.0, 1.0);
          // Pop effect: scale to 1.2 briefly when threshold is first crossed
          // during an active drag.
          final iconScale = (_isDragging && _thresholdCrossed)
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
              // Reply icon fills the gap and is right-aligned within it
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
                        child: widget.icon,
                      ),
                    ),
                  ),
                ),
              ),
              // The message bubble, translated right
              Transform.translate(offset: Offset(offset, 0), child: child),
            ],
          );
        },
        child: widget.child,
      ),
    );
  }
}
