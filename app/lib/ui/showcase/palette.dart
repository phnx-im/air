// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/ui/colors/palette.dart';
import 'package:air/ui/colors/themes.dart';

class PaletteShowcase extends StatelessWidget {
  const PaletteShowcase({super.key});

  Widget _buildRow(
    BuildContext context,
    String name,
    MaterialColor swatch,
    List<int> shades,
  ) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            name,
            style: TextStyle(
              color: CustomColorScheme.of(context).text.quaternary,
            ),
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: shades.map((shade) {
              final color = swatch[shade]!;
              final textColor = color.computeLuminance() > 0.5
                  ? Colors.black
                  : Colors.white;
              return Tooltip(
                message: 'Copy value',
                child: MouseRegion(
                  cursor: SystemMouseCursors.click,
                  child: GestureDetector(
                    onTap: () {
                      Clipboard.setData(
                        ClipboardData(
                          text:
                              '#${color.toARGB32().toRadixString(16).padLeft(8, '0').toUpperCase()}',
                        ),
                      );
                    },
                    child: Container(
                      width: 40,
                      height: 40,
                      alignment: Alignment.bottomLeft,
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(4),
                        color: color,
                      ),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 6,
                          vertical: 2,
                        ),
                        child: Text(
                          shade.toString(),
                          style: TextStyle(fontSize: 8, color: textColor),
                        ),
                      ),
                    ),
                  ),
                ),
              );
            }).toList(),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    // shades for neutrals include extra stops
    const neutralShades = [
      0,
      25,
      50,
      100,
      150,
      200,
      300,
      400,
      500,
      600,
      700,
      800,
      850,
      900,
      950,
      975,
      1000,
    ];
    const accentShades = [
      50,
      100,
      150,
      200,
      300,
      400,
      500,
      600,
      700,
      800,
      850,
      900,
      950,
    ];

    return SingleChildScrollView(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildRow(context, 'Neutral', AppColors.neutral, neutralShades),
          _buildRow(context, 'Red', AppColors.red, accentShades),
          _buildRow(context, 'Orange', AppColors.orange, accentShades),
          _buildRow(context, 'Yellow', AppColors.yellow, accentShades),
          _buildRow(context, 'Green', AppColors.green, accentShades),
          _buildRow(context, 'Cyan', AppColors.cyan, accentShades),
          _buildRow(context, 'Blue', AppColors.blue, accentShades),
          _buildRow(context, 'Purple', AppColors.purple, accentShades),
          _buildRow(context, 'Magenta', AppColors.magenta, accentShades),
        ],
      ),
    );
  }
}
