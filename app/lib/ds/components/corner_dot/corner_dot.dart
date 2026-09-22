// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/corner_dot/corner_dot_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Pins a small circular badge to the top-right corner of [child] when [show]
/// is true, and returns [child] untouched otherwise.
///
/// The composer's unread mark and the chat header's back-button mark are the
/// same badge, so they are the same widget.
class CornerDot extends StatelessWidget {
  const CornerDot({super.key, required this.show, required this.child});

  final bool show;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (!show) return child;
    return Stack(
      clipBehavior: .none,
      children: [
        child,
        Positioned(
          top: CornerDotTokens.insetTop,
          right: CornerDotTokens.insetRight,
          child: Container(
            width: CornerDotTokens.size,
            height: CornerDotTokens.size,
            decoration: BoxDecoration(
              color: SemanticPalette.of(context).function.neutral.toggleBlack,
              shape: .circle,
            ),
          ),
        ),
      ],
    );
  }
}
