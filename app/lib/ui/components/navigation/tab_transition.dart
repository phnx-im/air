// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

const double _tabSwitchSlideOffset = 16;

/// Transition builder for [AnimatedSwitcher] when swapping primary tab
/// content. Fades the incoming child in while sliding it up by
/// [_tabSwitchSlideOffset] logical pixels.
Widget tabSwitchTransition(Widget child, Animation<double> animation) {
  return FadeTransition(
    opacity: animation,
    child: AnimatedBuilder(
      animation: animation,
      builder: (context, child) {
        final dy = _tabSwitchSlideOffset * (1 - animation.value);
        return Transform.translate(offset: Offset(0, dy), child: child);
      },
      child: child,
    ),
  );
}
