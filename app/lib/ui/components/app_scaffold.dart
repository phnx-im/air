// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';

class AppScaffold extends StatelessWidget {
  const AppScaffold({super.key, this.title, required this.child});

  final String? title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        leading: const AppBarBackButton(),
        title: title != null
            ? Text(title!, maxLines: 1, overflow: TextOverflow.ellipsis)
            : null,
      ),
      body: SafeArea(
        minimum: const EdgeInsets.only(
          left: Spacings.s,
          right: Spacings.s,
          bottom: 40,
        ),
        child: child,
      ),
    );
  }
}
