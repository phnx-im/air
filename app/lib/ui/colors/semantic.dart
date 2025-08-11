// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

class BackGroundBaseColors {
  final Color primary;
  final Color secondary;
  final Color tertiary;
  final Color quaternary;

  BackGroundBaseColors({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class BackGroundElevatedColors {
  final Color primary;
  final Color secondary;
  final Color tertiary;
  final Color quaternary;

  BackGroundElevatedColors({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class TextColors {
  final Color primary;
  final Color secondary;
  final Color tertiary;
  final Color quaternary;

  TextColors({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class SeparatorColors {
  final Color primary;
  final Color secondary;

  SeparatorColors({required this.primary, required this.secondary});
}

class FillColors {
  final Color primary;
  final Color secondary;
  final Color tertiary;

  FillColors({
    required this.primary,
    required this.secondary,
    required this.tertiary,
  });
}

class FunctionColors {
  final Color white;
  final Color black;
  final Color toggleWhite;
  final Color toggleBlack;
  final Color success;
  final Color warning;
  final Color danger;
  final Color link;

  FunctionColors({
    required this.white,
    required this.black,
    required this.toggleWhite,
    required this.toggleBlack,
    required this.success,
    required this.warning,
    required this.danger,
    required this.link,
  });
}

class MessageColors {
  final Color selfBackground;
  final Color otherBackground;
  final Color selfText;
  final Color otherText;

  MessageColors({
    required this.selfBackground,
    required this.otherBackground,
    required this.selfText,
    required this.otherText,
  });
}
