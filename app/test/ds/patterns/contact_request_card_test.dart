// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/contact_request_card/contact_request_card.dart';
import 'package:air/ds/patterns/contact_request_card/contact_request_card_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('ContactRequestCard', () {
    Widget buildSubject(ContactRequestCardActions actions) => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      home: Scaffold(
        body: ContactRequestCard(
          tokens: ContactRequestCardTokens.current,
          title: 'Contact request',
          displayName: 'Alice',
          image: MemoryImage(transparentPixelPng),
          pictureRevealLabel: 'Tap to reveal their picture',
          message: 'Hi, it is Alice',
          messageLabel: 'Their message',
          messageRevealLabel: 'Tap to reveal their message',
          actions: actions,
        ),
      ),
    );

    testWidgets('covers the picture and the note of a request to the reader', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRequestAnswers(
            acceptLabel: 'Accept',
            declineLabel: 'Decline',
            blockLabel: 'Block',
            onAccept: () {},
            onDecline: () {},
            onBlock: () {},
          ),
        ),
      );

      expect(find.text('Tap to reveal their picture'), findsOneWidget);
      expect(find.text('Hi, it is Alice'), findsNothing);

      await tester.tap(find.text('Tap to reveal their message'));
      await tester.pump();
      expect(find.text('Hi, it is Alice'), findsOneWidget);
    });

    testWidgets('holds declining and blocking while accepting', (tester) async {
      final answered = <String>[];
      ContactRequestAnswers answers({required bool isAccepting}) =>
          ContactRequestAnswers(
            acceptLabel: 'Accept',
            declineLabel: 'Decline',
            blockLabel: 'Block',
            onAccept: () => answered.add('accept'),
            onDecline: () => answered.add('decline'),
            onBlock: () => answered.add('block'),
            isAccepting: isAccepting,
          );

      await tester.pumpWidget(buildSubject(answers(isAccepting: true)));
      await tester.tap(find.text('Decline'), warnIfMissed: false);
      await tester.tap(find.text('Block'), warnIfMissed: false);
      await tester.pump();
      expect(answered, isEmpty);

      await tester.pumpWidget(buildSubject(answers(isAccepting: false)));
      await tester.tap(find.text('Decline'));
      await tester.tap(find.text('Block'));
      await tester.pump();
      expect(answered, ['decline', 'block']);
    });

    testWidgets("shows the picture and the note of the reader's own request", (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRequestRetraction(label: 'Retract request', onRetract: () {}),
        ),
      );

      expect(find.text('Tap to reveal their picture'), findsNothing);
      expect(find.text('Tap to reveal their message'), findsNothing);
      expect(find.text('Hi, it is Alice'), findsOneWidget);
    });
  });
}
