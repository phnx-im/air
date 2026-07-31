// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Primitives', () {
    // The accessors throw on a missing cell, which is only unreachable while
    // both grids stay rectangular. Walking the full cross product is what
    // keeps that a fact rather than an intention.
    test('defines every neutral shade', () {
      for (final shade in NeutralShade.values) {
        expect(
          () => Primitives.neutral(shade),
          returnsNormally,
          reason: 'neutral/${shade.name} is missing',
        );
      }
    });

    test('defines every chromatic hue at every shade', () {
      for (final hue in Hue.values) {
        for (final shade in Shade.values) {
          expect(
            () => Primitives.chromatic(hue, shade),
            returnsNormally,
            reason: '${hue.name}/${shade.name} is missing',
          );
        }
      }
    });

    test('holds no duplicate colors within a hue', () {
      for (final hue in Hue.values) {
        final seen = <int, Shade>{};
        for (final shade in Shade.values) {
          final value = Primitives.chromatic(hue, shade).toARGB32();
          expect(
            seen[value],
            isNull,
            reason:
                '${hue.name}/${shade.name} duplicates '
                '${hue.name}/${seen[value]?.name}',
          );
          seen[value] = shade;
        }
      }
    });

    test('neutral runs from pure white to pure black', () {
      expect(Primitives.neutral(NeutralShade.s0).toARGB32(), 0xFFFFFFFF);
      expect(Primitives.neutral(NeutralShade.s1000).toARGB32(), 0xFF000000);
    });

    test('every shade is fully opaque', () {
      for (final shade in NeutralShade.values) {
        expect(Primitives.neutral(shade).a, 1.0);
      }
      for (final hue in Hue.values) {
        for (final shade in Shade.values) {
          expect(Primitives.chromatic(hue, shade).a, 1.0);
        }
      }
    });
  });
}
