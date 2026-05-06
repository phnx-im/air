// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/components/navigation/navigation_tokens.dart';
import 'package:air/widgets/edge_fade.dart';
import 'package:flutter/material.dart';

/// Scrollable screen chrome with a fixed header and soft fade edges.
class FadedScrollFrame extends StatelessWidget {
  const FadedScrollFrame({
    super.key,
    required this.header,
    required this.builder,
    required this.backgroundColor,
    this.topFadeHeight = 96,
    this.bottomFadeHeight = 120,
    this.contentTopPadding,
    this.bottomInset,
  });

  final Widget header;

  final Widget Function(double topPadding, double bottomPadding) builder;

  final Color backgroundColor;
  final double topFadeHeight;
  final double bottomFadeHeight;

  final double? contentTopPadding;

  final double? bottomInset;

  @override
  Widget build(BuildContext context) {
    final topPadding = contentTopPadding ?? kToolbarHeight;
    final tabBarInset = TabBarTokens.bottomInset(context);
    final bottomPadding =
        bottomInset ??
        (tabBarInset > bottomFadeHeight ? tabBarInset : bottomFadeHeight);

    return Container(
      color: backgroundColor,
      child: Stack(
        children: [
          Positioned.fill(child: builder(topPadding, bottomPadding)),
          Positioned.fill(
            bottom: null,
            child: EdgeFade(
              edge: FadeEdge.top,
              height: topFadeHeight,
              color: backgroundColor,
              curve: Curves.easeInOutQuad,
              solidStop: 0.3,
            ),
          ),
          Positioned.fill(bottom: null, child: header),
          Positioned.fill(
            top: null,
            child: EdgeFade(
              edge: FadeEdge.bottom,
              height: bottomFadeHeight,
              color: backgroundColor,
              curve: Curves.easeInOutQuad,
              solidStop: 0.1,
            ),
          ),
        ],
      ),
    );
  }
}
