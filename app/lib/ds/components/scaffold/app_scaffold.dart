// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:flutter/material.dart';

// Gap between the top of the content and the bottom of the app bar
const double _contentGap = S.s40;

// Diameter of the back button's visible circle, ButtonIcon's default size
// rather than its wider hit target.
const double _backButtonSize = S.s40;

class AppScaffold extends StatelessWidget {
  const AppScaffold({
    super.key,
    this.title,
    this.onTitleLongPress,
    this.backgroundColor,
    this.trailing,
    this.scrollable = true,
    this.reserveWindowControls = false,
    required this.child,
  });

  final String? title;
  final Function()? onTitleLongPress;
  final Color? backgroundColor;

  /// Pinned to the app bar's trailing edge.
  final Widget? trailing;

  /// Whether the scaffold scrolls [child] as one block. A child scrolling its
  /// own content takes the body's height instead, so a list is not laid out in
  /// full to be scrolled past.
  final bool scrollable;

  /// Whether the back button clears the native window controls floating over
  /// the window's top-left. Only a screen reaching that corner needs it, so
  /// the host owns the choice.
  final bool reserveWindowControls;

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final leadingInset = reserveWindowControls
        ? Chrome.windowControlsInset
        : S.s16;

    return Scaffold(
      backgroundColor: backgroundColor,
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        clipBehavior: .none,
        backgroundColor: backgroundColor,
        leading: AppBarBackButton(leadingInset: leadingInset),
        // The bar clips its leading slot to this, so it carries the inset as
        // well as the button.
        leadingWidth: leadingInset + _backButtonSize,
        title: title != null
            ? GestureDetector(
                onLongPress: onTitleLongPress,
                child: Text(
                  title!,
                  maxLines: 1,
                  overflow: .ellipsis,
                  style: const TextStyle(fontWeight: .bold),
                ),
              )
            : null,
        // Null rather than an empty list, so a bar with no action lays out as
        // it did before there was a slot for one.
        actions: switch (trailing) {
          null => null,
          final trailing => [
            Padding(
              padding: const EdgeInsets.only(right: S.s8),
              child: trailing,
            ),
          ],
        },
      ),
      body: SafeArea(
        minimum: EdgeInsets.only(
          left: S.s16,
          right: S.s16,
          bottom: scrollable ? _contentGap : 0,
        ),
        child: scrollable ? _scrolled() : child,
      ),
    );
  }

  Widget _scrolled() => LayoutBuilder(
    builder: (context, constraints) => SingleChildScrollView(
      padding: const EdgeInsets.only(top: _contentGap),
      child: ConstrainedBox(
        // Content that centers or fills needs the viewport to measure
        // against, which the unbounded scroll view alone does not give.
        constraints: BoxConstraints(
          minHeight: constraints.maxHeight - _contentGap,
        ),
        child: child,
      ),
    ),
  );
}
