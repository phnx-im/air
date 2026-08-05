// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scroll/scroll_edges.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/widgets.dart';

/// Which edge of the fade zone is fully opaque.
enum FadeEdge { top, bottom }

/// A vertical fade, strongest at [edge] and clear at the far side, painted as
/// a gradient.
///
/// [curve] shapes the ramp between the two ends. [solidStop] (0..1) holds a
/// band next to [edge] at its peak before the ramp starts, and that peak is
/// [opacity] while the far end lands at zero.
class EdgeFade extends StatelessWidget {
  const EdgeFade({
    super.key,
    required this.edge,
    required this.height,
    required this.color,
    this.curve = Curves.linear,
    this.solidStop = 0.0,
    this.opacity = 1.0,
  }) : assert(solidStop >= 0.0 && solidStop < 1.0),
       assert(opacity >= 0.0 && opacity <= 1.0);

  final FadeEdge edge;
  final double height;
  final Color color;
  final Curve curve;
  final double solidStop;

  /// Alpha at the solid edge, scaling the gradient as a whole. Below 1 the
  /// content stays partly visible even where the fade is strongest. We bake it
  /// into the gradient's stops rather than use an [Opacity] layer, which would
  /// cost a save layer per frame.
  final double opacity;

  static const int _steps = 32;

  @override
  Widget build(BuildContext context) {
    final solid = color.withValues(alpha: opacity);
    final stops = <double>[0.0];
    final colors = <Color>[solid];
    if (solidStop > 0) {
      stops.add(solidStop);
      colors.add(solid);
    }
    for (var i = 1; i <= _steps; i++) {
      final t = i / _steps;
      stops.add(solidStop + (1.0 - solidStop) * t);
      colors.add(color.withValues(alpha: opacity * (1.0 - curve.transform(t))));
    }
    final (begin, end) = switch (edge) {
      FadeEdge.top => (Alignment.topCenter, Alignment.bottomCenter),
      FadeEdge.bottom => (Alignment.bottomCenter, Alignment.topCenter),
    };
    return IgnorePointer(
      child: Container(
        height: height,
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: begin,
            end: end,
            stops: stops,
            colors: colors,
          ),
        ),
      ),
    );
  }
}

/// Cross-fades [child] out while the content rests against [edge].
///
/// An [EdgeFade] only belongs on an end that has content beyond it: at rest the
/// outermost row sits on the same solid surface the fade paints, so fading it
/// there washes it out and hides whatever chrome shares the strip. A null
/// [edges] leaves the fade painted.
class EdgeFadeReveal extends StatelessWidget {
  const EdgeFadeReveal({
    super.key,
    required this.edges,
    required this.edge,
    required this.child,
  });

  final ValueListenable<ScrollEdges>? edges;
  final FadeEdge edge;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final edges = this.edges;
    if (edges == null) return child;
    return ValueListenableBuilder<ScrollEdges>(
      valueListenable: edges,
      child: child,
      builder: (context, value, child) {
        final atRest = switch (edge) {
          FadeEdge.top => value.atTop,
          FadeEdge.bottom => value.atBottom,
        };
        return AnimatedOpacity(
          opacity: atRest ? 0 : 1,
          duration: Effect.duration(MotionPreset.short),
          curve: Effect.easeOutQuart,
          child: child,
        );
      },
    );
  }
}
