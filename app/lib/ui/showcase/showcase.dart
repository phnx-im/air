// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/showcase/context_menu.dart';
import 'package:air/ui/showcase/grid.dart';
import 'package:air/ui/showcase/palette.dart';
import 'package:air/ui/showcase/typescale.dart';
import 'package:air/ui/theme/font.dart';
import 'package:air/ui/theme/scale.dart';

void main() => runApp(const ShowcaseApp());

class ShowcaseApp extends StatelessWidget {
  const ShowcaseApp({super.key});

  @override
  Widget build(BuildContext context) {
    final scalingFactors = getScalingFactors(context);

    final materialApp = MaterialApp(
      title: 'UI Component Showcase',
      theme: buildThemeData(context),
      home: const ShowcaseHome(),
      debugShowCheckedModeBanner: false,
      builder: (context, child) => Stack(
        fit: StackFit.expand,
        children: [
          if (child != null) child,
          const GridOverlayInteractive(gridSize: 4),
        ],
      ),
    );

    // Add text scaling
    final app = MediaQuery(
      data: MediaQuery.of(
        context,
      ).copyWith(textScaler: TextScaler.linear(scalingFactors.textFactor)),
      child: materialApp,
    );

    // Add UI scaling
    return FractionallySizedBox(
      widthFactor: 1 / scalingFactors.uiFactor,
      heightFactor: 1 / scalingFactors.uiFactor,
      child: Transform.scale(scale: scalingFactors.uiFactor, child: app),
    );
  }
}

class ShowcaseHome extends StatelessWidget {
  const ShowcaseHome({super.key});

  @override
  Widget build(BuildContext context) {
    return DefaultTabController(
      length: 3, // Number of components to showcase
      child: Scaffold(
        appBar: AppBar(
          title: const Text('UI Component Showcase'),
          bottom: const TabBar(
            tabs: [
              Tab(text: 'Typescale'),
              Tab(text: 'Palette'),
              Tab(text: 'Context menu'),
            ],
          ),
        ),
        body: const TabBarView(
          children: [
            Center(child: Typescale()),
            Center(child: PaletteShowcase()),
            Center(child: ContextMenuShowcase()),
          ],
        ),
      ),
    );
  }
}

ThemeData buildThemeData(BuildContext context) {
  return ThemeData(
    useMaterial3: true,
    colorScheme: lightColorScheme,
    textTheme: customTextScheme,
  );
}
