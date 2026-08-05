// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// Which ends of a scrollable's content rest against the viewport.
///
/// An edge fade only belongs on an end that has content beyond it, so the two
/// flags gate the two strips. Value equality keeps a [ValueNotifier] of these
/// quiet during a scroll that doesn't cross an edge.
@immutable
class ScrollEdges {
  const ScrollEdges({required this.atTop, required this.atBottom});

  /// Content shorter than the viewport rests against both ends at once.
  factory ScrollEdges.of(ScrollMetrics metrics) => ScrollEdges(
    atTop: metrics.pixels <= metrics.minScrollExtent + _epsilon,
    atBottom: metrics.pixels >= metrics.maxScrollExtent - _epsilon,
  );

  /// Offsets within this many pixels of an end count as resting against it.
  /// Overscroll and sub-pixel jitter must not flip a fade on.
  static const double _epsilon = 1;

  /// The state of an unscrolled list, before it has any metrics to report.
  static const ScrollEdges atRest = ScrollEdges(atTop: true, atBottom: false);

  final bool atTop;
  final bool atBottom;

  @override
  bool operator ==(Object other) =>
      other is ScrollEdges &&
      other.atTop == atTop &&
      other.atBottom == atBottom;

  @override
  int get hashCode => Object.hash(atTop, atBottom);
}
