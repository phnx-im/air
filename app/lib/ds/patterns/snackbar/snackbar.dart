// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:flutter/widgets.dart';

/// Compact pill carrying the brief "it happened" feedback for an action that
/// needs no follow-up, such as a copy or a save.
///
/// Text only: a leading glyph and an action stay reserved for an undo-style
/// prompt, so a passive confirmation and something that wants a tap never blur
/// together. Placement, entry motion, and auto-dismiss belong to whatever
/// presents the pill.
class Snackbar extends StatelessWidget {
  const Snackbar({
    super.key,
    required this.tokens,
    required this.label,
    this.tone = SnackbarTone.neutral,
  });

  final SnackbarTokens tokens;
  final String label;
  final SnackbarTone tone;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    final fill = switch (tone) {
      SnackbarTone.neutral => palette.function.success.primary,
      SnackbarTone.danger => palette.function.danger,
    };

    return ConstrainedBox(
      constraints: BoxConstraints(maxWidth: tokens.maxWidth),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: fill,
          borderRadius: BorderRadius.circular(tokens.radius),
          boxShadow: Effect.elevation(tokens.elevation),
        ),
        child: Padding(
          padding: tokens.padding,
          child: Text(
            label,
            maxLines: 1,
            softWrap: false,
            overflow: .ellipsis,
            // Both fills are saturated in either brightness, so the label
            // takes the mode-invariant white rather than a toggling one.
            style: typeScale.body.s
                .style(color: palette.function.neutral.white)
                .copyWith(height: 1.2),
          ),
        ),
      ),
    );
  }
}
