// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/nux/nux_scaffold_tokens.dart';
import 'package:flutter/material.dart' show Material;
import 'package:flutter/widgets.dart';

/// The frame every signed-out screen sits in.
///
/// One frame, so all screens share one column: a small top row, the screen's
/// mark centered under it, and the actions at the bottom. Carries the ink
/// surface a Material descendant looks for.
class NuxScaffold extends StatelessWidget {
  const NuxScaffold({
    super.key,
    required this.tokens,
    this.top,
    required this.body,
    this.footer,
    this.overlay,
  });

  final NuxScaffoldTokens tokens;

  /// The screen's top row.
  final Widget? top;

  /// The screen's own content, capped to the column.
  final Widget body;

  /// The actions, pinned to the bottom of the column.
  final Widget? footer;

  /// Floats over the top-left corner instead of taking a row, so the screen's
  /// mark centers in the full height.
  final Widget? overlay;

  @override
  Widget build(BuildContext context) {
    Widget capped(Widget child, double maxWidth) => ConstrainedBox(
      constraints: BoxConstraints(maxWidth: maxWidth),
      child: child,
    );

    Widget content = Padding(
      padding: tokens.windowPadding,
      child: Column(
        children: [
          if (top case final top?)
            Align(alignment: tokens.topAlignment, child: top),
          Expanded(child: _body(context, capped)),
          if (footer case final footer?)
            Center(child: capped(footer, tokens.contentMaxWidth)),
        ],
      ),
    );

    if (overlay case final overlay?) {
      content = Stack(
        children: [
          content,
          Positioned(
            left: tokens.overlayInset,
            top: tokens.overlayInset,
            child: overlay,
          ),
        ],
      );
    }

    return Material(
      type: .canvas,
      color: NuxScaffoldTokens.surface(context),
      child: SafeArea(
        // No scaffold resizes for the keyboard here. Insetting the whole
        // column keeps the footer above the keyboard.
        child: Padding(
          padding: EdgeInsets.only(
            bottom: MediaQuery.viewInsetsOf(context).bottom,
          ),
          child: content,
        ),
      ),
    );
  }

  /// The body, capped to the column and centered in the room it is given.
  Widget _body(BuildContext context, Widget Function(Widget, double) capped) {
    final column = Center(child: capped(body, tokens.contentMaxWidth));

    return LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          child: column,
        ),
      ),
    );
  }
}
