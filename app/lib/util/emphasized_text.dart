// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/gestures.dart';
import 'package:flutter/painting.dart';

/// First of the private use code points standing in for a value.
const int _markerBase = 0xE000;

/// A value interpolated into a localized message.
class EmphasizedValue {
  const EmphasizedValue(this.text, {this.recognizer});

  final String text;

  /// Attached to the span the value lands in, where the value acts on a tap.
  /// The caller owns it, since a recognizer outlives the build that reads it.
  final GestureRecognizer? recognizer;
}

/// Formats a localized message and emphasizes the values it interpolates.
///
/// [format] is handed one marker per entry in [values] and returns the
/// finished message, which is then split on those markers. The values never
/// reach the formatter, so whatever a display name contains it cannot move the
/// emphasis or the text around it.
///
/// The point of going through markers is that the message stays one whole
/// sentence in the ARB files. Each language puts the values where it needs
/// them, and the emphasis and any gesture follow them there.
TextSpan emphasizedText(
  String Function(List<String> markers) format,
  List<EmphasizedValue> values,
  TextStyle style,
) {
  final markers = [
    for (var index = 0; index < values.length; index++)
      String.fromCharCode(_markerBase + index),
  ];

  final spans = <InlineSpan>[];
  final literal = StringBuffer();

  void flushLiteral() {
    if (literal.isNotEmpty) {
      spans.add(TextSpan(text: literal.toString()));
      literal.clear();
    }
  }

  for (final rune in format(markers).runes) {
    final index = rune - _markerBase;
    if (index >= 0 && index < values.length) {
      flushLiteral();
      final value = values[index];
      spans.add(
        TextSpan(text: value.text, style: style, recognizer: value.recognizer),
      );
    } else {
      literal.writeCharCode(rune);
    }
  }
  flushLiteral();

  return TextSpan(children: spans);
}
