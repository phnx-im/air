// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// [PageTransitionsBuilder] that mirrors [CupertinoPageTransitionsBuilder] and
/// adds a translucent scrim over the outgoing route, bacause Cupertino's native
/// parallax does not dim the underlying page. We need this because we use the
/// same background color on e.g. the chat list and the message list.
class CupertinoScrimPageTransitionsBuilder extends PageTransitionsBuilder {
  const CupertinoScrimPageTransitionsBuilder();

  @override
  Widget buildTransitions<T>(
    PageRoute<T> route,
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) {
    final cupertino = const CupertinoPageTransitionsBuilder().buildTransitions(
      route,
      context,
      animation,
      secondaryAnimation,
      child,
    );
    return Stack(
      children: [
        Positioned.fill(
          child: IgnorePointer(
            child: FadeTransition(
              opacity: animation,
              child: ColoredBox(color: Colors.black.withValues(alpha: 0.2)),
            ),
          ),
        ),
        cupertino,
      ],
    );
  }
}
