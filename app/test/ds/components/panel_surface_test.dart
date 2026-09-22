// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  /// Pumps [probe] under an optional published surface and returns what it saw.
  Future<T> read<T>(
    WidgetTester tester,
    T Function(BuildContext context) probe, {
    Color? surface,
  }) async {
    late T seen;
    Widget child = Builder(
      builder: (context) {
        seen = probe(context);
        return const SizedBox.shrink();
      },
    );
    if (surface != null) child = PanelSurface(color: surface, child: child);
    await tester.pumpWidget(Directionality(textDirection: .ltr, child: child));
    return seen;
  }

  group('PanelSurface.colorOf', () {
    testWidgets('falls back to the base tier with no shell', (tester) async {
      final color = await read(tester, PanelSurface.colorOf);
      expect(color, lightSemanticPalette.backgroundBase.primary);
    });

    testWidgets('reads the published surface', (tester) async {
      const published = Color(0xFF123456);
      final color = await read(
        tester,
        PanelSurface.colorOf,
        surface: published,
      );
      expect(color, published);
    });
  });

  group('PanelSurface.textOf', () {
    testWidgets('blends the text slots onto the published surface', (
      tester,
    ) async {
      const published = Color(0xFF123456);
      final text = await read(tester, PanelSurface.textOf, surface: published);

      // Opaque, so a color emoji in the run keeps its own colors.
      for (final color in [
        text.primary,
        text.secondary,
        text.tertiary,
        text.quaternary,
      ]) {
        expect(color.a, 1.0, reason: '$color');
      }
      expect(
        text.tertiary.toARGB32(),
        lightSemanticPalette.text.tertiary.on(published).toARGB32(),
      );
    });

    testWidgets('falls back to the base tier with no shell', (tester) async {
      final text = await read(tester, PanelSurface.textOf);
      final palette = lightSemanticPalette;
      expect(
        text.primary.toARGB32(),
        palette.text.primary.on(palette.backgroundBase.primary).toARGB32(),
      );
    });
  });
}
