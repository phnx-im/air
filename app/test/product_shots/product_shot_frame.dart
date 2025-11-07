// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:flutter/material.dart';

class ProductShotFrame extends StatelessWidget {
  const ProductShotFrame({
    super.key,
    required this.child,
    required this.statusBar,
    required this.screenSize,
    required this.statusBarHeight,
    required this.devicePixelRatio,
    required this.safeArea,
    this.borderWidth = 32,
    this.cornerRadius = 80,
    this.frameColor = Colors.black,
  });

  final Widget child;
  final Widget statusBar;
  final Size screenSize;
  final double statusBarHeight;
  final double devicePixelRatio;
  final EdgeInsets safeArea;
  final double borderWidth;
  final double cornerRadius;
  final Color frameColor;

  @override
  Widget build(BuildContext context) {
    final innerRadius = math.max(cornerRadius - borderWidth, 0.0);
    final baseMediaQuery =
        MediaQuery.maybeOf(context) ?? const MediaQueryData();

    final mediaQuery = baseMediaQuery.copyWith(
      size: screenSize,
      padding: safeArea,
      viewPadding: safeArea,
      viewInsets: EdgeInsets.zero,
      devicePixelRatio: devicePixelRatio,
      systemGestureInsets: EdgeInsets.zero,
    );

    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(cornerRadius),
        boxShadow: const [
          BoxShadow(
            color: Color(0x33000000),
            blurRadius: 64,
            offset: Offset(0, 0),
          ),
        ],
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(cornerRadius),
        clipBehavior: Clip.antiAlias,
        child: ColoredBox(
          color: frameColor,
          child: Padding(
            padding: EdgeInsets.all(borderWidth),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(innerRadius),
              clipBehavior: Clip.antiAlias,
              child: SizedBox(
                width: screenSize.width,
                height: screenSize.height,
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    MediaQuery(data: mediaQuery, child: child),
                    if (statusBarHeight > 0)
                      Positioned(
                        top: 0,
                        left: 0,
                        right: 0,
                        child: SizedBox(
                          height: statusBarHeight,
                          child: statusBar,
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
