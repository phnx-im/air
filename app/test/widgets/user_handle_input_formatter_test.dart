// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/widgets/user_handle_input_formatter.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const formatter = UserHandleInputFormatter();

  TextEditingValue value(String text) => TextEditingValue(
    text: text,
    selection: TextSelection.collapsed(offset: text.length),
  );

  test('allows valid usernames and lowercases characters', () {
    final result = formatter.formatEditUpdate(
      TextEditingValue.empty,
      value('Hello-World'),
    );
    expect(result.text, 'hello-world');
  });

  test('rejects usernames starting with a digit', () {
    final result = formatter.formatEditUpdate(
      TextEditingValue.empty,
      value('1abc'),
    );
    expect(result.text, isEmpty);
  });

  test('rejects consecutive dashes', () {
    final first = formatter.formatEditUpdate(
      TextEditingValue.empty,
      value('jo-'),
    );
    expect(first.text, 'jo-');

    final second = formatter.formatEditUpdate(first, value('jo--'));
    expect(second.text, 'jo-');
  });

  test('rejects unsupported characters such as underscores', () {
    final first = formatter.formatEditUpdate(
      TextEditingValue.empty,
      value('jo_'),
    );
    expect(first.text, isEmpty);
  });

  test('allows underscores when configured', () {
    const underscoreFormatter = UserHandleInputFormatter(allowUnderscore: true);
    final result = underscoreFormatter.formatEditUpdate(
      TextEditingValue.empty,
      value('user_name'),
    );
    expect(result.text, 'user_name');
  });

  test('rejects usernames longer than 63 characters', () {
    final valid = 'a' * 63;
    final result = formatter.formatEditUpdate(
      TextEditingValue.empty,
      value(valid),
    );
    expect(result.text, valid);

    final tooLongResult = formatter.formatEditUpdate(
      result,
      value('${valid}a'),
    );
    expect(tooLongResult.text, valid);
  });

  test('normalize respects validation rules', () {
    expect(UserHandleInputFormatter.normalize('1abc'), isEmpty);
    expect(UserHandleInputFormatter.normalize('valid-name'), 'valid-name');
    expect(
      UserHandleInputFormatter.normalize('foo_bar', allowUnderscore: true),
      'foo_bar',
    );
  });
}
