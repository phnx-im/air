// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

double figmaToFlutterBlurRadius(double val) {
  return val * 0.57735; // Convert to Flutter's blur radius
}

const Color lightModeShadowColor = Color(0x16000000);
const Color darkModeShadowColor = Color(0x80000000);

List<BoxShadow> elevationBoxShadows(BuildContext context) {
  final color = Theme.of(context).brightness == Brightness.dark
      ? darkModeShadowColor
      : lightModeShadowColor;
  return [
    BoxShadow(
      color: color,
      offset: const Offset(0, 1),
      blurRadius: 0,
      spreadRadius: 0,
    ),
    BoxShadow(
      color: color,
      offset: const Offset(0, 40),
      blurRadius: figmaToFlutterBlurRadius(80),
      spreadRadius: 0,
    ),
  ];
}
