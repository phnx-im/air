// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/counter/counter_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Pill carrying a count, e.g. a chat's unread messages.
///
/// A pure view: geometry comes from [tokens], color from the palette.
class Counter extends StatelessWidget {
  const Counter({super.key, required this.tokens, required this.count});

  final CounterTokens tokens;
  final int count;

  @override
  Widget build(BuildContext context) {
    if (count < 1) {
      return const SizedBox.shrink();
    }

    final palette = SemanticPalette.of(context);
    // Counts are unbounded, so past three digits the pill would keep growing
    // into the space the preview text needs.
    final label = count <= 100 ? '$count' : '100+';

    return Container(
      constraints: BoxConstraints(minWidth: tokens.minWidth),
      height: tokens.height,
      padding: tokens.padding,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: palette.fill.secondary,
        borderRadius: BorderRadius.circular(CounterTokens.radius),
      ),
      child: Text(
        label,
        style: typeScale.body.mini
            .style(color: palette.text.primary, weight: Weight.emphasized)
            .copyWith(height: 1.0),
      ),
    );
  }
}
