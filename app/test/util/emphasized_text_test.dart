// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/util/emphasized_text.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';

const _style = TextStyle(fontWeight: FontWeight.bold);

/// The spans of [span], as the text each carries and whether it is emphasized.
List<(String, bool)> runs(TextSpan span) => [
  for (final child in span.children ?? const <InlineSpan>[])
    ((child as TextSpan).text ?? '', child.style == _style),
];

List<EmphasizedValue> values(List<String> texts) => [
  for (final text in texts) EmphasizedValue(text),
];

void main() {
  group('emphasizedText', () {
    test('emphasizes the values and leaves the message around them plain', () {
      final span = emphasizedText(
        (marks) => '${marks[0]} added ${marks[1]}',
        values(['Alice', 'Bob']),
        _style,
      );

      expect(runs(span), [('Alice', true), (' added ', false), ('Bob', true)]);
      expect(span.toPlainText(), 'Alice added Bob');
    });

    test('follows the values where a translation moves them', () {
      final span = emphasizedText(
        (marks) => '${marks[1]} wurde von ${marks[0]} hinzugefügt',
        values(['Alice', 'Bob']),
        _style,
      );

      expect(runs(span), [
        ('Bob', true),
        (' wurde von ', false),
        ('Alice', true),
        (' hinzugefügt', false),
      ]);
    });

    test('emphasizes every occurrence of a repeated value', () {
      final span = emphasizedText(
        (marks) => '${marks[0]} left, so ${marks[0]} is gone',
        values(['Alice']),
        _style,
      );

      expect(runs(span), [
        ('Alice', true),
        (' left, so ', false),
        ('Alice', true),
        (' is gone', false),
      ]);
    });

    test('a value carrying a marker cannot split the message', () {
      final span = emphasizedText(
        (marks) => '${marks[0]} created the group',
        values(['\u{E000}Mallory']),
        _style,
      );

      expect(runs(span), [
        ('\u{E000}Mallory', true),
        (' created the group', false),
      ]);
    });

    test('keeps text outside the basic plane intact', () {
      final span = emphasizedText(
        (marks) => '${marks[0]} 👋 🇸🇪',
        values(['Alice']),
        _style,
      );

      expect(runs(span), [('Alice', true), (' 👋 🇸🇪', false)]);
    });

    test('a message with no values is one plain run', () {
      final span = emphasizedText(
        (marks) => 'This client has been onboarded.',
        const [],
        _style,
      );

      expect(runs(span), [('This client has been onboarded.', false)]);
    });

    test('a recognizer rides along with the value it belongs to', () {
      final recognizer = TapGestureRecognizer();
      addTearDown(recognizer.dispose);

      final span =
          emphasizedText((marks) => '${marks[0]} renamed it to ${marks[1]}', [
            EmphasizedValue('Alice', recognizer: recognizer),
            const EmphasizedValue('Ops'),
          ], _style);

      final recognizers = [
        for (final child in span.children!) (child as TextSpan).recognizer,
      ];
      expect(recognizers, [recognizer, null, null]);
    });
  });
}
