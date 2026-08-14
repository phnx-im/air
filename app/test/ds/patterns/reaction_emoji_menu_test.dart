// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/emoji/centered_emoji.dart';
import 'package:air/ds/patterns/reaction_emoji_menu/reaction_emoji_menu.dart';
import 'package:air/ds/patterns/reaction_emoji_menu/reaction_emoji_menu_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _helpLabel = 'Sets your default skin tone';
const _sectionTitle = 'People';
const _baseGlyph = '\u{270B}';
const _tones = [_baseGlyph, '\u{270B}\u{1F3FB}'];

void main() {
  group('ReactionEmojiMenu', () {
    Widget buildSubject() => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      home: Scaffold(
        body: ReactionEmojiMenu(
          tokens: ReactionEmojiMenuTokens.phone,
          sections: const [
            EmojiMenuSection(
              title: _sectionTitle,
              emojis: [EmojiMenuEntry(glyph: _baseGlyph, tones: _tones)],
            ),
          ],
          searchHint: 'Search',
          emptyLabel: 'No results',
          tone: EmojiMenuTone(
            options: _tones,
            selected: 0,
            helpLabel: _helpLabel,
            onSelected: (_) {},
          ),
        ),
      ),
    );

    /// The style [finder]'s text is painted in, which is what a glyph in the
    /// flyout resolves against too.
    TextStyle ambientStyle(WidgetTester tester, Finder finder) =>
        DefaultTextStyle.of(tester.element(finder)).style;

    testWidgets('paints the tone flyout in the menu\'s own text style', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(buildSubject());

      final inMenu = ambientStyle(
        tester,
        find.text(_sectionTitle.toUpperCase()),
      );

      // The header's tone button, which comes before the grid's cells.
      await tester.tap(find.byType(CenteredEmoji).first);
      await tester.pumpAndSettle();

      final inFlyout = ambientStyle(tester, find.text(_helpLabel));

      // The flyout is a route of its own. Without a surface over it, its text
      // and its swatches take the app's fallback style, which underlines them.
      expect(inFlyout.decoration, isNot(TextDecoration.underline));
      expect(inFlyout.fontFamily, inMenu.fontFamily);
    });
  });
}
