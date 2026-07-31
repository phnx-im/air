// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/util/anchored_list/height_cache.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('retains historical height estimate when active items are removed', () {
    final cache = AnchoredListHeightCache(defaultHeight: 50);

    cache.setHeight(1, 100);
    cache.setHeight(2, 300);
    expect(cache.averageHeight, 200);
    expect(cache.totalHeight, 400);

    cache.remove(2);

    expect(cache.totalHeight, 100);
    expect(cache.averageHeight, 200);
    expect(cache.getHeight(2), 50);
  });

  test('damps a tall outlier across the estimate window', () {
    final cache = AnchoredListHeightCache(defaultHeight: 50);

    for (var id = 0; id < 100; id++) {
      cache.setHeight(id, 100);
    }
    expect(cache.averageHeight, 100);

    // One image among a hundred text rows barely moves the estimate.
    cache.setHeight(100, 500);
    expect(cache.averageHeight, closeTo(103.96, 0.01));
  });

  test('re-measuring the same heights leaves the average unchanged', () {
    // Heights are recorded during layout, and layout can run several passes
    // per frame over the same children. If re-measuring moved the average,
    // the scroll extent estimate would differ between passes of one frame
    // and the viewport would keep correcting itself.
    final cache = AnchoredListHeightCache(defaultHeight: 50);

    for (var id = 0; id < 20; id++) {
      cache.setHeight(id, id.isEven ? 40 : 900);
    }
    final afterFirstPass = cache.averageHeight;

    for (var pass = 0; pass < 5; pass++) {
      for (var id = 0; id < 20; id++) {
        cache.setHeight(id, id.isEven ? 40 : 900);
      }
      expect(cache.averageHeight, afterFirstPass);
    }
  });

  test('caps retained historical estimates', () {
    final cache = AnchoredListHeightCache(
      defaultHeight: 50,
      maxRetainedEstimates: 2,
    );

    cache.setHeight(1, 100);
    cache.setHeight(2, 300);
    expect(cache.averageHeight, 200);

    cache.setHeight(3, 500);

    expect(cache.averageHeight, 400);
  });
}
