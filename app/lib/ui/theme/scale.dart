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
  const linux = 14.0;

  const iosUiTweak = 1.0;
  const androidUiTweak = 1.0;
  const macosUiTweak = 1.0;
  const windowsUiTweak = 1.0;
  const linuxUiTweak = 1.0;

  const iosTextTweak = 1.0;
  const androidTextTweak = 1.0;
  const macosTextTweak = 1.1;
  const windowsTextTweak = 1.0;
  const linuxTextTweak = 1.0;

  const refBase = ios;

  if (Platform.isIOS) {
    return const ScalingFactors(
      uiFactor: ios / refBase * iosUiTweak,
      textFactor: iosTextTweak,
    );
  } else if (Platform.isMacOS) {
    return const ScalingFactors(
      uiFactor: macos / refBase * macosUiTweak,
      textFactor: macosTextTweak,
    );
  } else if (Platform.isAndroid) {
    const uiFactor = android / android * androidUiTweak;
    assert(
      uiFactor == 1.0,
      "Android UI scaling factor must be 1.0, "
      "in particular because of paddings to avoid system UI.",
    );
    return const ScalingFactors(
      uiFactor: uiFactor,
      textFactor: androidTextTweak,
    );
  } else if (Platform.isWindows) {
    return const ScalingFactors(
      uiFactor: windows / refBase * windowsUiTweak,
      textFactor: windowsTextTweak,
    );
  } else if (Platform.isLinux) {
    return const ScalingFactors(
      uiFactor: linux / refBase * linuxUiTweak,
      textFactor: linuxTextTweak,
    );
  } else {
    return const ScalingFactors(uiFactor: 1.0, textFactor: 1.0);
  }
}

double actualTextSize(double fontSize, BuildContext context) {
  final scalingFactors = getScalingFactors(context);
  return fontSize * scalingFactors.textFactor;
}

double actualUiSize(double size, BuildContext context) {
  final scalingFactors = getScalingFactors(context);
  return size * scalingFactors.uiFactor;
}
