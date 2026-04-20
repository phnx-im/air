// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/widgets/anchored_list/height_cache.dart';
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

  test('dampens extent estimate changes after warmup', () {
    final cache = AnchoredListHeightCache(
      defaultHeight: 50,
      estimateWarmupSamples: 3,
      estimateSmoothingFactor: 0.25,
    );

    cache.setHeight(1, 100);
    cache.setHeight(2, 100);
    cache.setHeight(3, 100);
    expect(cache.averageHeight, 100);

    cache.setHeight(4, 500);

    // The raw historical average would jump to 200 here. We damp the update
    // so the scrollbar estimate changes more gradually as new items are
    // measured during scrolling.
    expect(cache.averageHeight, 125);
  });

  test('caps retained historical estimates', () {
    final cache = AnchoredListHeightCache(
      defaultHeight: 50,
      estimateWarmupSamples: 10,
      maxRetainedEstimates: 2,
    );

    cache.setHeight(1, 100);
    cache.setHeight(2, 300);
    expect(cache.averageHeight, 200);

    cache.setHeight(3, 500);

    expect(cache.averageHeight, 400);
  });
}
