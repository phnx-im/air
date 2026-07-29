// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/foundations/dimensions.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:flutter/material.dart';

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
        minimum: const EdgeInsets.only(left: S.s16, right: S.s16, bottom: 40),
        child: child,
      ),
    );
  }
}
