// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:air/ds/patterns/message_text/message_text.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/message_list/message_details.dart';
import 'package:air/features/message_list/message_renderer.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';
import '../chat_list/chat_list_content_test.dart';

final _versions = [
  UiMessageVersion(
    content: textContent('See you at 6'),
    createdAt: DateTime(2026, 10, 29, 14, 2),
  ),
  UiMessageVersion(
    content: textContent('See you at 7'),
    createdAt: DateTime(2026, 10, 29, 14, 5),
  ),
  UiMessageVersion(
    content: textContent('See you at 7:30'),
    createdAt: DateTime(2026, 10, 29, 14, 41),
  ),
];

/// Stands in for the chat row: an incoming bubble with the current text.
class _Preview extends StatelessWidget {
  const _Preview();

  @override
  Widget build(BuildContext context) {
    final blocks = [
      for (final inner in textContent('See you at 7:30').content!.elements)
        buildBlockElement(context, inner.element, false),
    ];
    return Align(
      alignment: Alignment.centerLeft,
      child: MessageBubble(
        isSelf: false,
        child: MessageText(isSelf: false, blocks: blocks),
      ),
    );
  }
}

void main() {
  Widget buildApp(Widget home) => Builder(
    builder: (context) => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: home,
    ),
  );

  Widget buildModal(Future<List<UiMessageVersion>> versions) => buildApp(
    MessageDetailsModal(preview: const _Preview(), versions: versions),
  );

  Widget buildSheet(Future<List<UiMessageVersion>> versions) => buildApp(
    Scaffold(
      body: Align(
        alignment: Alignment.bottomCenter,
        child: SingleChildScrollView(
          child: MessageDetailsSheet(
            preview: const _Preview(),
            versions: versions,
          ),
        ),
      ),
    ),
  );

  group('MessageDetailsModal', () {
    testWidgets('lists the versions newest first under the message', (
      tester,
    ) async {
      await tester.pumpWidget(buildModal(Future.value(_versions)));
      await tester.pumpAndSettle();

      double topOf(Finder finder) => tester.getTopLeft(finder).dy;

      final current = find.text('See you at 7:30');
      final sent = find.textContaining('Sent on Oct 29, 2026');
      final edited = find.textContaining('Edited on Oct 29, 2026');
      expect(current, findsNWidgets(2));
      expect(sent, findsOneWidget);
      expect(edited, findsNWidgets(2));

      // The preview leads, then each version sits above its caption with the
      // original at the bottom.
      final order = [
        current.at(0),
        find.text('Edit history'),
        current.at(1),
        edited.at(0),
        find.text('See you at 7'),
        edited.at(1),
        find.text('See you at 6'),
        sent,
      ];
      final tops = [for (final finder in order) topOf(finder)];
      expect(tops, orderedEquals([...tops]..sort()));
    });

    testWidgets('leaves out the edit history of an unedited message', (
      tester,
    ) async {
      await tester.pumpWidget(buildModal(Future.value(_versions.sublist(2))));
      await tester.pumpAndSettle();

      expect(find.byType(_Preview), findsOneWidget);
      expect(find.text('Edit history'), findsNothing);
      expect(find.textContaining('Sent on'), findsNothing);
    });

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildModal(Future.value(_versions)));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_details.png'),
      );
    });

    testWidgets('lines the captions up with the text in the bubble', (
      tester,
    ) async {
      await tester.pumpWidget(buildModal(Future.value(_versions)));
      await tester.pumpAndSettle();

      double leftOf(Finder finder) => tester.getTopLeft(finder).dx;

      final text = leftOf(find.text('See you at 6'));
      final captions = find.textContaining('on Oct 29, 2026');
      expect(captions, findsNWidgets(3));
      for (final caption in captions.evaluate()) {
        expect(leftOf(find.byWidget(caption.widget)), text);
      }
    });

    testWidgets(
      'paints every version in the outgoing fill on the card',
      variant: desktopPlatform,
      (tester) async {
        sizeView(tester, desktopViewSize);
        tester.platformDispatcher.platformBrightnessTestValue = .dark;
        addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
        await tester.pumpWidget(buildModal(Future.value(_versions)));
        await tester.pumpAndSettle();

        final palette = SemanticPalette.of(
          tester.element(find.byType(MessageDetailsView)),
        );
        // The preview leads with its own incoming bubble.
        final fills = [
          for (final bubble in find.byType(MessageBubble).evaluate().skip(1))
            (tester
                        .widget<Container>(
                          find
                              .descendant(
                                of: find.byWidget(bubble.widget),
                                matching: find.byType(Container),
                              )
                              .first,
                        )
                        .decoration!
                    as BoxDecoration)
                .color,
        ];
        expect(fills, hasLength(3));
        expect(
          fills,
          everyElement(
            palette.fill.secondary.on(palette.backgroundElevated.primary),
          ),
        );
      },
    );

    testWidgets('reports versions that fail to load', (tester) async {
      final versions = Completer<List<UiMessageVersion>>();
      await tester.pumpWidget(buildModal(versions.future));
      versions.completeError(Exception('database is gone'));
      await tester.pumpAndSettle();

      expect(find.byType(_Preview), findsOneWidget);
      expect(
        find.text('Failed to load message details. Try again.'),
        findsOneWidget,
      );
    });
  });

  group('MessageDetailsSheet', () {
    testWidgets('renders correctly', (tester) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(buildSheet(Future.value(_versions)));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_details_sheet.png'),
      );
    });

    testWidgets('closes from its header', (tester) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(
        buildApp(
          Builder(
            builder: (context) => TextButton(
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => Scaffold(
                    body: MessageDetailsSheet(
                      preview: const _Preview(),
                      versions: Future.value(_versions),
                    ),
                  ),
                ),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      expect(find.text('Message details'), findsOneWidget);

      await tester.tap(find.byType(DialogHeaderAction));
      await tester.pumpAndSettle();
      expect(find.text('Message details'), findsNothing);
    });
  });
}
