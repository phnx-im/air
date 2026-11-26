// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/mention_pill/mention_pill.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

WidgetSpan _pill(String name) => WidgetSpan(
  alignment: .baseline,
  baseline: .alphabetic,
  child: MentionPill(name: name),
);

void main() {
  testWidgets('pills on consecutive lines keep their fills apart', (
    tester,
  ) async {
    await tester.pumpWidget(
      Directionality(
        textDirection: .ltr,
        child: Align(
          alignment: .topLeft,
          child: SizedBox(
            width: 100,
            child: Text.rich(
              TextSpan(
                children: [
                  _pill('Simone'),
                  const TextSpan(text: ' '),
                  _pill('Konrad'),
                ],
              ),
            ),
          ),
        ),
      ),
    );

    final fills = find.descendant(
      of: find.byType(MentionPill),
      matching: find.byType(DecoratedBox),
    );
    final first = tester.getRect(fills.at(0));
    final second = tester.getRect(fills.at(1));

    expect(second.top, greaterThan(first.top), reason: 'pills must wrap');
    expect(second.top - first.bottom, greaterThanOrEqualTo(1));
  });
}
