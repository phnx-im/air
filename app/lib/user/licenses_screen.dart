// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';

class LicensesScreen extends StatelessWidget {
  const LicensesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(loc.licensesScreen_title),
        leading: const AppBarBackButton(),
      ),
      body: const SafeArea(
        child: Padding(
          padding: EdgeInsets.all(Spacings.s),
          child: Text("TODO"),
        ),
      ),
    );
  }
}
