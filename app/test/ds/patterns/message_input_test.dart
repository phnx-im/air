// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/corner_dot/corner_dot_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_input/message_input.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('MessageInput', () {
    Widget buildSubject({
      MessageInputTokens tokens = MessageInputTokens.phone,
      bool showSend = false,
      bool showScrollBack = false,
      bool scrollBackUnread = false,
      AppIconType leadingIcon = AppIconType.plus,
      AppIconType sendIcon = AppIconType.arrowUp,
      List<Widget> aboveField = const [],
      void Function(BuildContext)? onLeading,
      VoidCallback? onSend,
      VoidCallback? onScrollBack,
    }) => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      home: Scaffold(
        body: Align(
          alignment: Alignment.bottomCenter,
          child: MessageInput(
            tokens: tokens,
            field: const TextField(
              decoration: InputDecoration(
                isCollapsed: true,
                contentPadding: EdgeInsets.zero,
              ),
            ),
            aboveField: aboveField,
            leadingIcon: leadingIcon,
            onLeading: onLeading ?? (_) {},
            sendIcon: sendIcon,
            showSend: showSend,
            onSend: onSend ?? () {},
            showScrollBack: showScrollBack,
            scrollBackUnread: scrollBackUnread,
            onScrollBack: onScrollBack ?? () {},
          ),
        ),
      ),
    );

    Finder buttonWith(AppIconType icon) =>
        find.byWidgetPredicate((w) => w is ButtonIcon && w.icon == icon);

    /// The unread dot, found by the shape and fill it is the only wearer of.
    final dot = find.byWidgetPredicate((w) {
      if (w is! Container) return false;
      final decoration = w.decoration;
      return decoration is BoxDecoration &&
          decoration.shape == BoxShape.circle &&
          decoration.color == lightSemanticPalette.function.neutral.toggleBlack;
    });

    /// A hidden trailing slot does not mount its button at all, so presence is
    /// what tells revealed from hidden.
    testWidgets('empty input shows only the leading button', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(buttonWith(AppIconType.plus), findsOneWidget);
      expect(buttonWith(AppIconType.arrowUp), findsNothing);
      expect(buttonWith(AppIconType.chevronDown), findsNothing);
    });

    testWidgets('send is revealed once there is something to send', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(showSend: true));
      await tester.pumpAndSettle();

      expect(buttonWith(AppIconType.arrowUp), findsOneWidget);
    });

    testWidgets('send wins the trailing slot from scroll-back', (tester) async {
      await tester.pumpWidget(buildSubject(showScrollBack: true));
      await tester.pumpAndSettle();
      expect(buttonWith(AppIconType.chevronDown), findsOneWidget);

      await tester.pumpWidget(
        buildSubject(showScrollBack: true, showSend: true),
      );
      await tester.pumpAndSettle();

      expect(buttonWith(AppIconType.arrowUp), findsOneWidget);
      expect(buttonWith(AppIconType.chevronDown), findsNothing);
    });

    testWidgets('send fades and grows in rather than popping into place', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.pumpWidget(buildSubject(showSend: true));
      await tester.pump(const Duration(milliseconds: 1));

      final fade = find.ancestor(
        of: buttonWith(AppIconType.arrowUp),
        matching: find.byType(Opacity),
      );
      final enteringOpacity = tester.widget<Opacity>(fade).opacity;
      // A painted rect composes the whole ancestor transform chain, so this is
      // the button's on-screen size rather than its layout size.
      final enteringWidth = tester
          .getRect(buttonWith(AppIconType.arrowUp))
          .width;

      await tester.pumpAndSettle();

      expect(enteringOpacity, greaterThan(0.0));
      expect(enteringOpacity, lessThan(1.0));
      expect(tester.widget<Opacity>(fade).opacity, 1.0);

      final size = MessageInputTokens.phone.buttonSize;
      expect(
        enteringWidth,
        greaterThanOrEqualTo(size * MessageInputTokens.sendEnterScale),
      );
      expect(enteringWidth, lessThan(size));
      expect(
        tester.getRect(buttonWith(AppIconType.arrowUp)).width,
        moreOrLessEquals(size),
      );
    });

    testWidgets('the field gives up width as the send slot opens', (
      tester,
    ) async {
      Finder fieldBox() => find
          .ancestor(
            of: find.byType(TextField),
            matching: find.byType(Container),
          )
          .first;

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      final wide = tester.getSize(fieldBox()).width;

      await tester.pumpWidget(buildSubject(showSend: true));
      await tester.pumpAndSettle();

      expect(
        tester.getSize(fieldBox()).width,
        wide - MessageInputTokens.phone.buttonSize - MessageInputTokens.gap,
      );
    });

    testWidgets('the unread dot rides on the scroll-back button only when '
        'there are unread messages', (tester) async {
      await tester.pumpWidget(buildSubject(showScrollBack: true));
      await tester.pumpAndSettle();
      expect(dot, findsNothing);

      await tester.pumpWidget(
        buildSubject(showScrollBack: true, scrollBackUnread: true),
      );
      await tester.pumpAndSettle();

      expect(dot, findsOneWidget);
      expect(tester.getSize(dot).width, CornerDotTokens.size);
    });

    testWidgets('the unread dot never rides on send', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          showScrollBack: true,
          scrollBackUnread: true,
          showSend: true,
        ),
      );
      await tester.pumpAndSettle();

      expect(buttonWith(AppIconType.arrowUp), findsOneWidget);
      expect(dot, findsNothing);
    });

    testWidgets('the buttons take the token diameter per density', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(showSend: true));
      await tester.pumpAndSettle();
      expect(
        tester.getSize(buttonWith(AppIconType.plus)).width,
        MessageInputTokens.phone.buttonSize,
      );

      await tester.pumpWidget(
        buildSubject(tokens: MessageInputTokens.desktop, showSend: true),
      );
      await tester.pumpAndSettle();
      expect(
        tester.getSize(buttonWith(AppIconType.plus)).width,
        MessageInputTokens.desktop.buttonSize,
      );
    });

    testWidgets('the field is at least as tall as the buttons', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      final field = find.ancestor(
        of: find.byType(TextField),
        matching: find.byType(Container),
      );
      expect(
        tester.getSize(field.first).height,
        MessageInputTokens.phone.buttonSize,
      );
    });

    testWidgets('rows above the field push the field down, not the buttons', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          aboveField: [
            const SizedBox(height: S.s40, child: Text('Edit message')),
          ],
        ),
      );
      await tester.pumpAndSettle();

      final leadingBottom = tester.getRect(buttonWith(AppIconType.plus)).bottom;
      final fieldBottom = tester
          .getRect(
            find
                .ancestor(
                  of: find.byType(TextField),
                  matching: find.byType(Container),
                )
                .first,
          )
          .bottom;

      // The row is bottom-anchored, so the banner grows the field upward and
      // the leading button stays put next to the field's last line.
      expect(leadingBottom, fieldBottom);
      expect(find.text('Edit message'), findsOneWidget);
    });

    testWidgets('gestures reach the host', (tester) async {
      var leading = 0;
      var send = 0;
      var scrollBack = 0;

      await tester.pumpWidget(
        buildSubject(
          showScrollBack: true,
          onLeading: (_) => leading++,
          onScrollBack: () => scrollBack++,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(buttonWith(AppIconType.plus));
      await tester.tap(buttonWith(AppIconType.chevronDown));

      await tester.pumpWidget(
        buildSubject(showSend: true, onSend: () => send++),
      );
      await tester.pumpAndSettle();
      await tester.tap(buttonWith(AppIconType.arrowUp));

      expect(leading, 1);
      expect(scrollBack, 1);
      expect(send, 1);
    });
  });
}
