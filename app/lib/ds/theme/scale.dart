// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/material.dart';

class ScalingFactors {
  final double uiFactor;
  final double textFactor;

  const ScalingFactors({required this.uiFactor, required this.textFactor});
}

ScalingFactors getScalingFactors(BuildContext context) {
  const ios = 17.0;
  const android = 16.0;
  const macos = 13.0;
  const windows = 15.0;
  const linux = 15.0;

  const refBase = ios;

  final mediaQuery = MediaQuery.of(context);
  final systemTextScale = mediaQuery.textScaler.scale(1.0);

  if (Platform.isIOS) {
    return ScalingFactors(uiFactor: ios / refBase, textFactor: systemTextScale);
  } else if (Platform.isMacOS) {
    return ScalingFactors(
      uiFactor: macos / refBase,
      textFactor: 1.1 * systemTextScale,
    );
  } else if (Platform.isAndroid) {
    const uiFactor = android / android;
    assert(
      uiFactor == 1.0,
      "Android UI scaling factor must be 1.0, "
      "in particular because of paddings to avoid system UI.",
    );
    return ScalingFactors(
      uiFactor: uiFactor,
      textFactor: 1.0 * systemTextScale,
    );
  } else if (Platform.isWindows) {
    return ScalingFactors(
      uiFactor: windows / refBase,
      textFactor: 1.0 * systemTextScale,
    );
  } else if (Platform.isLinux) {
    // On Linux, we never manually scale the text only to behave like other apps
    // like Firefox. Historically, there was no fine control of UI scaling
    // in GNOME for HiDPI, which is why there's today both UI scaling and
    // (in GNOME Tweaks) the legacy text scale factor still in use by some.
    return ScalingFactors(
      uiFactor: linux / refBase * systemTextScale,
      textFactor: 1.0,
    );
  } else {
    return ScalingFactors(uiFactor: 1.0, textFactor: 1.0 * systemTextScale);
  }
}

double actualUiSize(double size, BuildContext context) {
  final scalingFactors = getScalingFactors(context);
  return size * scalingFactors.uiFactor;
}
