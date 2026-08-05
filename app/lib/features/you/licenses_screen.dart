// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:flutter/material.dart';

class LicensesScreen extends StatelessWidget {
  const LicensesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        clipBehavior: .none,
        title: Text(loc.licensesScreen_title),
        leading: const AppBarBackButton(),
      ),
      body: const SafeArea(
        child: Padding(padding: EdgeInsets.all(S.s16), child: Text("TODO")),
      ),
    );
  }
}
