// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scroll/edge_fade.dart';
import 'package:air/ds/components/scroll/scroll_edges.dart';
import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/widgets.dart';

/// Scrollable screen chrome with a fixed header and soft fade edges.
///
/// The fade values are required rather than defaulted: the host owns them, and
/// a default here would be a second source for geometry its token bag already
/// names.
class FadedScrollFrame extends StatelessWidget {
  const FadedScrollFrame({
    super.key,
    required this.header,
    required this.builder,
    required this.backgroundColor,
    required this.contentTopPadding,
    required this.contentBottomPadding,
    required this.topFadeHeight,
    required this.bottomFadeHeight,
    required this.topSolidStop,
    required this.bottomSolidStop,
    required this.bottomOpacity,
    this.edges,
  });

  final Widget header;

  final Widget Function(double topPadding, double bottomPadding) builder;

  final Color backgroundColor;
  final double topFadeHeight;
  final double bottomFadeHeight;

  /// Fraction of each strip held at full strength before its ramp starts.
  final double topSolidStop;
  final double bottomSolidStop;

  /// Peak alpha of the bottom strip. The top one beds the header and is always
  /// opaque.
  final double bottomOpacity;

  /// Space kept above the first row, for the caller's [header] and whatever
  /// chrome floats over the top edge.
  final double contentTopPadding;

  /// Space kept below the last row, for whatever chrome floats over the bottom
  /// edge.
  final double contentBottomPadding;

  /// Which ends of the content rest against the viewport. With them, each fade
  /// shows only once there's content beyond the edge it guards: at rest the
  /// outermost row sits on the same solid surface the fade paints, so fading it
  /// there only washes it out. Without them, both always paint.
  final ValueListenable<ScrollEdges>? edges;

  @override
  Widget build(BuildContext context) {
    return Container(
      color: backgroundColor,
      child: Stack(
        children: [
          Positioned.fill(
            child: builder(contentTopPadding, contentBottomPadding),
          ),
          Positioned.fill(
            bottom: null,
            child: EdgeFadeReveal(
              edges: edges,
              edge: FadeEdge.top,
              child: EdgeFade(
                edge: FadeEdge.top,
                height: topFadeHeight,
                color: backgroundColor,
                curve: Curves.easeInOutQuad,
                solidStop: topSolidStop,
              ),
            ),
          ),
          Positioned.fill(bottom: null, child: header),
          Positioned.fill(
            top: null,
            child: EdgeFadeReveal(
              edges: edges,
              edge: FadeEdge.bottom,
              child: EdgeFade(
                edge: FadeEdge.bottom,
                height: bottomFadeHeight,
                color: backgroundColor,
                curve: Curves.easeInOutQuad,
                solidStop: bottomSolidStop,
                opacity: bottomOpacity,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
