// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/link_preview/link_preview.dart';
import 'package:air/ds/patterns/message_input/message_input.dart';
import 'package:air/ds/patterns/message_input/message_input_link_preview.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _url = 'https://air.ms/blog/link-previews';

const _data = LinkPreviewData(
  url: _url,
  title: 'Link previews in Air',
  description: 'How a card is resolved by the sender and travels inside.',
  imageWidth: 200,
  imageHeight: 200,
);

void main() {
  group('MessageInputLinkPreview', () {
    Widget buildSubject({LinkPreviewData? data, VoidCallback? onRemove}) =>
        MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testLightTheme,
          home: Scaffold(
            body: Align(
              alignment: Alignment.bottomCenter,
              child: MessageInput(
                tokens: MessageInputTokens.phone,
                field: const TextField(
                  decoration: InputDecoration(
                    isCollapsed: true,
                    contentPadding: EdgeInsets.zero,
                  ),
                ),
                aboveField: [
                  MessageInputLinkPreview(
                    url: _url,
                    data: data,
                    imageBuilder: (_) => const ColoredBox(color: Colors.teal),
                    onRemove: onRemove,
                  ),
                ],
                leadingIcon: AppIconType.plus,
                onLeading: (_) {},
                sendIcon: AppIconType.arrowUp,
                onSend: () {},
                onScrollBack: () {},
              ),
            ),
          ),
        );

    final removeButton = find.byWidgetPredicate(
      (w) => w is ButtonIcon && w.icon == AppIconType.x,
    );

    testWidgets('dismissing the preview reaches the host', (tester) async {
      var removed = 0;
      await tester.pumpWidget(
        buildSubject(data: _data, onRemove: () => removed++),
      );
      await tester.pumpAndSettle();
      await tester.tap(removeButton);

      expect(removed, 1);
    });

    testWidgets('offers no dismiss without a callback', (tester) async {
      await tester.pumpWidget(buildSubject(data: _data));
      await tester.pumpAndSettle();

      expect(find.text(_data.title), findsOneWidget);
      expect(removeButton, findsNothing);
    });

    testWidgets('offers no dismiss while pending', (tester) async {
      await tester.pumpWidget(buildSubject(onRemove: () {}));
      await tester.pumpAndSettle();

      expect(find.text(_url), findsOneWidget);
      expect(removeButton, findsNothing);
    });

    testWidgets('renders correctly', (tester) async {
      sizeView(tester, const Size(400, 320));
      await tester.pumpWidget(buildSubject(data: _data, onRemove: () {}));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_input_link_preview.png'),
      );
    });
  });
}
