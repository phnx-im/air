// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'package:flutter/material.dart';

// Ligatures to turn off in monospaced code. `calt` alone is not enough:
// repeated-punctuation ligatures (e.g. ```) are delivered through the
// standard ligature features `liga`/`clig` (and sometimes `dlig`) in
// Iosevka-based fonts such as Adwaita Mono, the default `monospace` on
// modern GNOME.
const disableLigaturesFontFeatures = [
  FontFeature.disable('liga'),
  FontFeature.disable('clig'),
  FontFeature.disable('calt'),
  FontFeature.disable('dlig'),
];

String getSystemMonospaceFontFamily() {
  if (Platform.isWindows) return 'Consolas';
  if (Platform.isMacOS || Platform.isIOS) return 'Menlo';
  if (Platform.isLinux) return 'monospace';
  if (Platform.isAndroid) return 'monospace';
  return 'monospace';
}

List<String>? getSystemMonospaceFontFallback() {
  return null;
}

extension SystemMonospaceTextStyle on TextStyle {
  TextStyle withSystemMonospace() => copyWith(
    fontFamily: getSystemMonospaceFontFamily(),
    fontFeatures: disableLigaturesFontFeatures,
    fontFamilyFallback: getSystemMonospaceFontFallback(),
  );
}
