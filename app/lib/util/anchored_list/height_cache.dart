// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:collection';

/// Caches measured heights of list items by their unique ID.
///
/// Maintains separate stores:
///
///  - **Exact cache** (`_heights`): heights of items currently in the data
///    model. Entries are added on layout and removed when items leave the
///    data. Used for precise offset calculations (layout correction, jump
///    targeting).
///
///  - **Estimate window** (`_estimatedHeights`): an LRU-bounded window of
///    the most recently measured heights (up to [maxRetainedEstimates]).
///    This window is *not* cleared when items leave the data, so the
///    average it produces remains stable as a paged chat window slides
///    between older and newer messages. Without this, the average would
///    oscillate as pages are swapped, causing scrollbar jitter.
///
/// [averageHeight] is derived from the estimate window using exponential
/// moving average (EMA) smoothing, which dampens short-term fluctuations
/// when a burst of unusually tall or short items is measured.
class AnchoredListHeightCache {
  AnchoredListHeightCache({
    this.defaultHeight = 50.0,
    this.estimateWarmupSamples = 8,
    this.estimateSmoothingFactor = 0.25,
    this.maxRetainedEstimates = 256,
  }) : _estimatedAverageHeight = defaultHeight;

  /// Fallback height for items never measured.
  final double defaultHeight;

  /// Number of samples before EMA smoothing kicks in. During warmup the
  /// average is computed directly to converge quickly from the default.
  final int estimateWarmupSamples;

  /// EMA alpha: 0.25 means ~25% weight on the new sample, ~75% on the
  /// running average. Lower values smooth more aggressively.
  final double estimateSmoothingFactor;

  /// Cap on the estimate window size. Oldest entries are evicted first.
  final int maxRetainedEstimates;

  /// Exact heights for items currently in the data model.
  final Map<Object, double> _heights = {};

  /// LRU-ordered window of recently measured heights for average estimation.
  /// [LinkedHashMap] gives insertion-order iteration for O(1) eviction.
  final LinkedHashMap<Object, double> _estimatedHeights =
      LinkedHashMap<Object, double>();
  double _totalHeight = 0;
  double _estimatedTotalHeight = 0;
  double _estimatedAverageHeight;

  int get cachedCount => _heights.length;
  double get totalHeight => _totalHeight;

  double get averageHeight =>
      _estimatedHeights.isNotEmpty ? _estimatedAverageHeight : defaultHeight;

  double getHeight(Object id) => _heights[id] ?? defaultHeight;

  double? lookupHeight(Object id) => _heights[id];

  /// Records a measured height for [id]. Updates both the exact cache and
  /// the estimate window, then refreshes the smoothed average.
  void setHeight(Object id, double height) {
    final old = _heights[id];
    _heights[id] = height;
    _totalHeight += height - (old ?? 0);

    // Move to the end of the LRU order (remove + re-insert).
    final estimatedOld = _estimatedHeights.remove(id);
    if (estimatedOld != null) {
      _estimatedTotalHeight -= estimatedOld;
    }
    _estimatedHeights[id] = height;
    _estimatedTotalHeight += height;
    _trimEstimatedHeights();
    _refreshAverageEstimate();
  }

  /// Removes [id] from the exact cache only. The estimate window
  /// deliberately retains the value so the average stays stable when
  /// items are paged out.
  void remove(Object id) {
    final old = _heights.remove(id);
    if (old != null) _totalHeight -= old;
  }

  void clear() {
    _heights.clear();
    _estimatedHeights.clear();
    _totalHeight = 0;
    _estimatedTotalHeight = 0;
    _estimatedAverageHeight = defaultHeight;
  }

  /// Evicts the oldest entries when the estimate window exceeds its cap.
  void _trimEstimatedHeights() {
    while (_estimatedHeights.length > maxRetainedEstimates) {
      final oldestId = _estimatedHeights.keys.first;
      final removed = _estimatedHeights.remove(oldestId);
      if (removed != null) {
        _estimatedTotalHeight -= removed;
      }
    }
  }

  /// Recomputes the smoothed average height.
  ///
  /// During warmup (≤ [estimateWarmupSamples]), uses the true average so
  /// the estimate converges quickly from [defaultHeight]. After warmup,
  /// applies EMA so that a sudden burst of tall/short items doesn't
  /// cause the scroll extent to jump.
  void _refreshAverageEstimate() {
    if (_estimatedHeights.isEmpty) {
      _estimatedAverageHeight = defaultHeight;
      return;
    }

    final targetAverage = _estimatedTotalHeight / _estimatedHeights.length;
    if (_estimatedHeights.length <= estimateWarmupSamples) {
      _estimatedAverageHeight = targetAverage;
      return;
    }

    // EMA: new_avg = old_avg + α × (sample_avg − old_avg)
    _estimatedAverageHeight +=
        (targetAverage - _estimatedAverageHeight) * estimateSmoothingFactor;
  }
}
