// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:flutter/material.dart';

// Gap between the top of the content and the bottom of the app bar
const double _contentGap = S.s40;

class AppScaffold extends StatelessWidget {
  const AppScaffold({
    super.key,
    this.title,
    this.onTitleLongPress,
    this.backgroundColor,
    required this.child,
  });

  final String? title;
  final Function()? onTitleLongPress;
  final Color? backgroundColor;

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: backgroundColor,
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        clipBehavior: Clip.none,
        backgroundColor: backgroundColor,
        leading: const AppBarBackButton(),
        title: title != null
            ? GestureDetector(
                onLongPress: onTitleLongPress,
                child: Text(
                  title!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontWeight: FontWeight.bold),
                ),
              )
            : null,
      ),
      body: SafeArea(
        minimum: const EdgeInsets.only(
          left: S.s16,
          right: S.s16,
          bottom: _contentGap,
        ),
        child: LayoutBuilder(
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
        ),
      ),
    );
  }
}
