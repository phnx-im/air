// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// The [InputDecoration] a DS field hands the `TextField` inside it.
abstract final class FieldChrome {
  /// Draws nothing at all, so the container the DS field paints is the only
  /// chrome there is.
  ///
  /// Every border is named rather than left to fall through: Material's own
  /// default is an underline, the ambient input theme supplies a fill and a
  /// border per state, and a bare [InputDecoration.border] does not override
  /// the per-state ones.
  static InputDecoration plain({String? hintText, TextStyle? hintStyle}) =>
      InputDecoration(
        isDense: true,
        contentPadding: EdgeInsets.zero,
        filled: false,
        border: InputBorder.none,
        enabledBorder: InputBorder.none,
        disabledBorder: InputBorder.none,
        focusedBorder: InputBorder.none,
        errorBorder: InputBorder.none,
        focusedErrorBorder: InputBorder.none,
        // A field that caps its length carries the remaining count in its own
        // helper line, so the built-in counter never renders.
        counterText: '',
        hintText: hintText,
        hintStyle: hintStyle,
      );
}
