// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/services.dart';

/// Formatter enforcing the canonical username syntax. Underscores can be allowed
/// temporarily for flows that still accept legacy usernames.
class UserHandleInputFormatter extends TextInputFormatter {
  const UserHandleInputFormatter({this.allowUnderscore = false});

  final bool allowUnderscore;

  static const int _maxLength = 63;
  static const int _dash = 45; // '-'
  static const int _underscore = 95; // '_'
  static const int _zero = 48;
  static const int _nine = 57;
  static const int _a = 97;
  static const int _z = 122;

  /// Normalizes raw input. Returns an empty string if the input violates the syntax.
  static String normalize(
    String input, {
    bool allowUnderscore = false,
  }) {
    final lower = input.trim().toLowerCase();
    return _isValid(lower, allowUnderscore: allowUnderscore) ? lower : '';
  }

  @override
  TextEditingValue formatEditUpdate(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    final lower = newValue.text.toLowerCase();
    if (!_isValid(lower, allowUnderscore: allowUnderscore)) {
      return oldValue;
    }

    return newValue.copyWith(
      text: lower,
      selection: TextSelection(
        baseOffset: _clampSelectionIndex(lower, newValue.selection.baseOffset),
        extentOffset: _clampSelectionIndex(lower, newValue.selection.extentOffset),
      ),
      composing: TextRange.empty,
    );
  }

  static int _clampSelectionIndex(String text, int offset) {
    if (offset < 0) {
      return 0;
    }
    if (offset > text.length) {
      return text.length;
    }
    return offset;
  }

  static bool _isValid(String text, {required bool allowUnderscore}) {
    if (text.length > _maxLength) {
      return false;
    }
    if (text.isEmpty) {
      return true;
    }
    var previousDash = false;
    for (var i = 0; i < text.length; i++) {
      final codeUnit = text.codeUnitAt(i);
      if (!_isAllowedChar(codeUnit, allowUnderscore: allowUnderscore)) {
        return false;
      }
      if (i == 0) {
        if (_isDigit(codeUnit) || codeUnit == _dash) {
          return false;
        }
      }
      if (codeUnit == _dash) {
        if (previousDash) {
          return false;
        }
        previousDash = true;
      } else {
        previousDash = false;
      }
    }
    return true;
  }

  static bool _isAllowedChar(int codeUnit, {required bool allowUnderscore}) {
    if (codeUnit == _dash) {
      return true;
    }
    if (allowUnderscore && codeUnit == _underscore) {
      return true;
    }
    if (_isDigit(codeUnit)) {
      return true;
    }
    return codeUnit >= _a && codeUnit <= _z;
  }

  static bool _isDigit(int codeUnit) =>
      codeUnit >= _zero && codeUnit <= _nine;
}
