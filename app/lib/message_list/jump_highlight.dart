// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:flutter/widgets.dart';

/// Provides the stream of jump target IDs to descendants.
class JumpHighlightScope extends InheritedWidget {
  const JumpHighlightScope({
    super.key,
    required this.jumpedToId,
    required super.child,
  });

  final Stream<Object> jumpedToId;

  static Stream<Object>? maybeOf(BuildContext context) {
    return context
        .dependOnInheritedWidgetOfExactType<JumpHighlightScope>()
        ?.jumpedToId;
  }

  @override
  bool updateShouldNotify(JumpHighlightScope oldWidget) =>
      jumpedToId != oldWidget.jumpedToId;
}

/// Renders the bubble background and a highlight when its [id] matches the most
/// recent jump target emitted by [JumpHighlightScope].
class JumpHighlight extends StatefulWidget {
  const JumpHighlight({
    super.key,
    required this.id,
    required this.borderRadius,
    required this.baseColor,
    required this.child,
  });

  final Object id;
  final BorderRadius borderRadius;
  final Color baseColor;
  final Widget child;

  @override
  State<JumpHighlight> createState() => _JumpHighlightState();
}

class _JumpHighlightState extends State<JumpHighlight>
    with SingleTickerProviderStateMixin {
  // Time to hold the highlight
  static const _holdMillis = 1000;
  // Time to fade back to the base color after the hold
  static const _fadeMillis = 2000;
  static const _totalDuration = Duration(
    milliseconds: _holdMillis + _fadeMillis,
  );

  // Cap on how far we lerp toward the highlight color
  static const double _peakStrength = 1;

  late final AnimationController _controller;
  late final Animation<double> _animation;
  StreamSubscription<Object>? _subscription;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: _totalDuration,
      value: 1.0,
    );
    _animation = TweenSequence<double>([
      TweenSequenceItem(
        tween: ConstantTween(_peakStrength),
        weight: _holdMillis.toDouble(),
      ),
      TweenSequenceItem(
        tween: Tween<double>(
          begin: _peakStrength,
          end: 0.0,
        ).chain(CurveTween(curve: Curves.easeOutCubic)),
        weight: _fadeMillis.toDouble(),
      ),
    ]).animate(_controller);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final stream = JumpHighlightScope.maybeOf(context);
    _subscription?.cancel();
    _subscription = stream?.where((id) => id == widget.id).listen((_) {
      _controller.forward(from: 0.0);
    });
  }

  @override
  void didUpdateWidget(covariant JumpHighlight oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.id != widget.id) {
      _controller.value = 1.0;
    }
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final highlightColor = CustomColorScheme.of(context).function.link;
    return AnimatedBuilder(
      animation: _animation,
      builder: (context, child) {
        final t = _animation.value;
        final glowing = t > 0;
        return Container(
          decoration: BoxDecoration(
            borderRadius: widget.borderRadius,
            color: widget.baseColor,
            boxShadow: glowing
                ? [
                    BoxShadow(
                      color: highlightColor.withValues(alpha: t),
                      spreadRadius: Spacing.px4,
                      blurRadius: Spacing.px24,
                    ),
                  ]
                : null,
          ),
          foregroundDecoration: glowing
              ? BoxDecoration(
                  borderRadius: widget.borderRadius,
                  border: Border.all(
                    color: highlightColor.withValues(
                      alpha: (t / _peakStrength).clamp(0.0, 1.0),
                    ),
                    width: 5,
                  ),
                )
              : null,
          child: child,
        );
      },
      child: widget.child,
    );
  }
}
