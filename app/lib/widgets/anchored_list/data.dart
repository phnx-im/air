// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:collection';

import 'package:flutter/foundation.dart';

/// Diff types emitted by [AnchoredListData] mutations.
///
/// Diffs are accumulated between listener notifications and drained by
/// the widget each frame via [AnchoredListData.drainDiffs]. The widget
/// uses them to decide whether a layout correction is needed and to
/// update the height cache.
sealed class AnchoredListDiff<T> {
  const AnchoredListDiff();
}

class AnchoredListInsert<T> extends AnchoredListDiff<T> {
  const AnchoredListInsert({
    required this.index,
    required this.count,
    required this.insertedItems,
  });

  final int index;
  final int count;
  final List<T> insertedItems;
}

class AnchoredListRemove<T> extends AnchoredListDiff<T> {
  const AnchoredListRemove({
    required this.index,
    required this.count,
    required this.removedItems,
  });

  final int index;
  final int count;
  final List<T> removedItems;
}

class AnchoredListUpdate<T> extends AnchoredListDiff<T> {
  const AnchoredListUpdate({required this.index, required this.oldItem});

  final int index;
  final T oldItem;
}

class AnchoredListReload<T> extends AnchoredListDiff<T> {
  const AnchoredListReload();
}

/// A [ChangeNotifier] that wraps a [List<T>] and tracks diffs.
///
/// Index 0 is the newest item (bottom of chat). This matches the reversed
/// scroll view: scroll offset 0 = index 0 = visually at the bottom.
///
/// Every mutation records a diff that the widget drains once per frame.
/// The diff describes what changed (insert/remove/update/reload) so the
/// widget can apply targeted height-cache updates and scroll corrections
/// without diffing the entire list.
class AnchoredListData<T> extends ChangeNotifier {
  AnchoredListData([List<T>? initial])
    : _items = initial != null ? List<T>.of(initial) : <T>[];

  final List<T> _items;
  final List<AnchoredListDiff<T>> _pendingDiffs = [];
  bool _batching = false;

  void _notify() {
    if (!_batching) notifyListeners();
  }

  /// Apply multiple mutations as a single atomic change.
  ///
  /// Listeners are notified once after the callback returns, so
  /// [drainDiffs] yields all diffs from the batch together.
  void batch(void Function() callback) {
    _batching = true;
    try {
      callback();
    } finally {
      _batching = false;
    }
    notifyListeners();
  }

  UnmodifiableListView<T> get items => UnmodifiableListView(_items);

  int get length => _items.length;

  T operator [](int index) => _items[index];

  /// Insert a single item at [index].
  void insert(int index, T item) {
    RangeError.checkValidIndex(index, _items, 'index', _items.length + 1);
    _items.insert(index, item);
    _pendingDiffs.add(
      AnchoredListInsert<T>(index: index, count: 1, insertedItems: [item]),
    );
    _notify();
  }

  /// Insert multiple items starting at [index].
  void insertAll(int index, List<T> items) {
    RangeError.checkValidIndex(index, _items, 'index', _items.length + 1);
    _items.insertAll(index, items);
    _pendingDiffs.add(
      AnchoredListInsert<T>(
        index: index,
        count: items.length,
        insertedItems: List<T>.of(items),
      ),
    );
    _notify();
  }

  /// Remove the item at [index].
  T removeAt(int index) {
    RangeError.checkValidIndex(index, _items);
    final removed = _items.removeAt(index);
    _pendingDiffs.add(
      AnchoredListRemove<T>(index: index, count: 1, removedItems: [removed]),
    );
    _notify();
    return removed;
  }

  /// Remove [count] items starting at [start].
  void removeRange(int start, int count) {
    RangeError.checkValidRange(start, start + count, _items.length);
    final removed = _items.sublist(start, start + count);
    _items.removeRange(start, start + count);
    _pendingDiffs.add(
      AnchoredListRemove<T>(index: start, count: count, removedItems: removed),
    );
    _notify();
  }

  /// Replace the item at [index] in place.
  void update(int index, T item) {
    RangeError.checkValidIndex(index, _items);
    final old = _items[index];
    _items[index] = item;
    _pendingDiffs.add(AnchoredListUpdate<T>(index: index, oldItem: old));
    _notify();
  }

  /// Replace the entire list. Clears pending diffs.
  ///
  /// The viewport stays at its current pixel offset.
  void reload(List<T> newItems) {
    _items
      ..clear()
      ..addAll(newItems);
    _pendingDiffs.clear();
    _pendingDiffs.add(const AnchoredListReload());
    _notify();
  }

  /// Returns and clears pending diffs. Called by the widget each frame.
  List<AnchoredListDiff<T>> drainDiffs() {
    if (_pendingDiffs.isEmpty) return const [];
    final diffs = List<AnchoredListDiff<T>>.of(_pendingDiffs);
    _pendingDiffs.clear();
    return diffs;
  }
}
