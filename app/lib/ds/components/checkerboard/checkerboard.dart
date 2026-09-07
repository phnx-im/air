// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/checkerboard/checkerboard_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// A two-tone grid of alternating color squares.
class Checkerboard extends StatelessWidget {
  const Checkerboard({super.key});

  @override
  Widget build(BuildContext context) {
    final fill = SemanticPalette.of(context).fill;
    return RepaintBoundary(
      child: CustomPaint(
        painter: _CheckerboardPainter(
          light: fill.tertiary,
          dark: fill.secondary,
          square: CheckerboardTokens.square,
        ),
      ),
    );
  }
}

class _CheckerboardPainter extends CustomPainter {
  const _CheckerboardPainter({
    required this.light,
    required this.dark,
    required this.square,
  });

  final Color light;
  final Color dark;
  final double square;

  @override
  void paint(Canvas canvas, Size size) {
    final bounds = Offset.zero & size;
    canvas.clipRect(bounds);
    canvas.drawRect(bounds, Paint()..color = light);

    final paint = Paint()..color = dark;
    final columns = (size.width / square).ceil();
    final rows = (size.height / square).ceil();
    for (var row = 0; row < rows; row++) {
      // Odd squares of even rows, even squares of odd rows.
      for (var column = row.isEven ? 1 : 0; column < columns; column += 2) {
        canvas.drawRect(
          Rect.fromLTWH(column * square, row * square, square, square),
          paint,
        );
      }
    }
  }

  @override
  bool shouldRepaint(_CheckerboardPainter oldDelegate) =>
      light != oldDelegate.light ||
      dark != oldDelegate.dark ||
      square != oldDelegate.square;
}
