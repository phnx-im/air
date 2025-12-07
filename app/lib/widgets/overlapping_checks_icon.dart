// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// ignore_for_file: prefer_const_constructors

import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';

class DoubleCheckIcon extends StatelessWidget {
  const DoubleCheckIcon({
    super.key,
    this.size = 16,
    this.borderWidth = 1,
    this.singleCheckIcon = false,
    this.inverted = false,
  });

  final double size;
  final double borderWidth;
  final double iconSizeRatio = 0.8;
  final bool singleCheckIcon;
  final bool inverted;

  @override
  Widget build(BuildContext context) {
    final systemForegroundColor = CustomColorScheme.of(context).text.tertiary;
    final systemBackgroundColor = CustomColorScheme.of(
      context,
    ).backgroundBase.primary;
    final color = inverted ? systemBackgroundColor : systemForegroundColor;
    final backgroundColor = inverted
        ? systemForegroundColor
        : systemBackgroundColor;

    return SizedBox(
      width: singleCheckIcon ? size : size * 1.5,
      height: size,
      child: Stack(
        children: [
          // Rear checkmark clipped to avoid overlap
          Positioned(
            left: 0,
            bottom: 0,
            child: Container(
              width: size,
              height: size,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                border: Border.all(color: color, width: borderWidth),
                color: backgroundColor,
              ),
              child: Center(
                child: Icon(
                  Icons.check,
                  color: color,
                  size: size * iconSizeRatio,
                ),
              ),
            ),
          ),
          // Front checkmark (fully visible)
          if (!singleCheckIcon)
            Positioned(
              left: size * 0.5,
              top: 0,
              child: Container(
                width: size,
                height: size,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(color: color, width: borderWidth),
                  color: backgroundColor,
                ),
                child: Center(
                  child: Icon(
                    Icons.check,
                    color: color,
                    size: size * iconSizeRatio,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
