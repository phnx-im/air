// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// Which edge of the fade zone is fully opaque.
enum FadeEdge { top, bottom }

/// A vertical fade that is fully opaque at [edge] and transparent at the
/// opposite edge, rendered as a gradient.
///
/// The fade is shaped by [curve] and may include an opaque region adjacent
/// to [edge] via [solidStop] (0..1). [curve] maps t=0 (solid edge) to
/// alpha=1 and t=1 (far edge) to alpha=0.
class EdgeFade extends StatelessWidget {
  const EdgeFade({
    super.key,
    required this.edge,
    required this.height,
    required this.color,
    this.curve = Curves.linear,
    this.solidStop = 0.0,
  }) : assert(solidStop >= 0.0 && solidStop < 1.0);

  final FadeEdge edge;
  final double height;
  final Color color;
  final Curve curve;
  final double solidStop;

  static const int _steps = 32;

  @override
  Widget build(BuildContext context) {
    final stops = <double>[0.0];
    final colors = <Color>[color];
    if (solidStop > 0) {
      stops.add(solidStop);
      colors.add(color);
    }
    for (var i = 1; i <= _steps; i++) {
      final t = i / _steps;
      stops.add(solidStop + (1.0 - solidStop) * t);
      colors.add(color.withValues(alpha: 1.0 - curve.transform(t)));
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
