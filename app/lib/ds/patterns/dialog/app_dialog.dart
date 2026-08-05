// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/dialog_tokens.dart';
import 'package:flutter/material.dart';

/// The centered dialog card, hosted on a [Dialog] route.
///
/// Material's [Dialog] is the host rather than the look: it lifts the card
/// clear of the keyboard, which matters for the dialogs that carry a field.
/// Every visible dimension comes from [DialogTokens].
class AppDialog extends StatelessWidget {
  const AppDialog({
    super.key,
    this.backgroundColor,
    this.maxWidth,
    required this.child,
  });

  final Widget child;
  final Color? backgroundColor;

  /// Widens the card past [DialogTokens.maxWidth] for content that needs it.
  final double? maxWidth;

  @override
  Widget build(BuildContext context) {
    final tokens = DialogTokens.current;
    final palette = SemanticPalette.of(context);

    return Dialog(
      backgroundColor: backgroundColor ?? palette.backgroundElevated.primary,
      insetPadding: tokens.margin,
      constraints: BoxConstraints(
        minWidth: tokens.minWidth,
        maxWidth: maxWidth ?? tokens.maxWidth,
      ),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(tokens.radius),
      ),
      child: Padding(padding: tokens.contentPadding, child: child),
    );
  }
}
