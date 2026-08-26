// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const white = Color(0xFFFFFFFF);
  const black = Color(0xFF000000);

  group('Color.on', () {
    test('composites the ink onto the background', () {
      const halfBlack = Color.from(alpha: 0.5, red: 0, green: 0, blue: 0);
      const halfWhite = Color.from(alpha: 0.5, red: 1, green: 1, blue: 1);
      const red = Color(0xFFFF0000);
      const clear = Color(0x00000000);

      final grey = halfBlack.on(white);
      expect(grey.r, closeTo(0.5, 1e-6));
      expect(grey.g, closeTo(0.5, 1e-6));
      expect(grey.b, closeTo(0.5, 1e-6));
      expect(halfWhite.on(black).r, closeTo(0.5, 1e-6));
      // Opaque ink is left alone; fully transparent ink is the background.
      expect(red.on(white).toARGB32(), red.toARGB32());
      expect(clear.on(white).toARGB32(), white.toARGB32());
      expect(clear.on(black).toARGB32(), black.toARGB32());
    });

    test('rejects a translucent background', () {
      final palette = lightSemanticPalette;
      expect(
        () => palette.text.tertiary.on(palette.fill.tertiary),
        throwsAssertionError,
      );
    });
  });

  group('TextPalette.on', () {
    List<Color> slots(TextPalette text) => [
      text.primary,
      text.secondary,
      text.tertiary,
      text.quaternary,
    ];

    // The text slots are the ones that hold emoji, so this is the property the
    // whole thing exists for: whatever the theme, no alpha is left over for an
    // emoji in the run to inherit.
    test('is fully opaque in both themes', () {
      for (final palette in [lightSemanticPalette, darkSemanticPalette]) {
        final onBase = palette.text.on(palette.backgroundBase.primary);
        for (final color in slots(onBase)) {
          expect(color.a, 1.0, reason: '$color');
        }
      }
    });

    test('blends every slot onto the background', () {
      final text = lightSemanticPalette.text;
      final blended = slots(text.on(white));
      final expected = slots(text).map((c) => c.on(white)).toList();
      for (var i = 0; i < blended.length; i++) {
        expect(blended[i].toARGB32(), expected[i].toARGB32());
      }
    });

    test('keeps the tiers ordered on a light surface', () {
      final luminances = slots(
        lightSemanticPalette.text.on(white),
      ).map((c) => c.computeLuminance());
      // primary is the strongest ink, so it stays the darkest.
      expect(luminances, orderedEquals(luminances.toList()..sort()));
    });

    test('memoizes per surface', () {
      final text = lightSemanticPalette.text;
      // Identity, not equality: a second read of the same surface must not
      // re-blend, which is what makes a per-row call cheap.
      expect(text.on(white), same(text.on(white)));
      expect(text.on(black), isNot(same(text.on(white))));
    });

    test('stays correct once the cache is capped and cleared', () {
      final text = lightSemanticPalette.text;
      // Walk more distinct surfaces than the cap holds, then re-read the first.
      for (var i = 0; i < 40; i++) {
        text.on(Color.fromARGB(0xFF, i, i, i));
      }
      final again = text.on(white);
      expect(again.primary.toARGB32(), text.primary.on(white).toARGB32());
      expect(again.primary.a, 1.0);
    });

    test('rejects a translucent background', () {
      final palette = lightSemanticPalette;
      expect(
        () => palette.text.on(palette.fill.tertiary),
        throwsAssertionError,
      );
    });
  });
}
