// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/emojis/generated.dart' as data;
import 'package:air/message_list/emoji_repository.dart';
import 'package:flutter_test/flutter_test.dart';

// Stable data points from the generated dataset.
const _grinning = '\u{1F600}'; // :grinning:
const _laughing = '\u{1F606}'; // :laughing: / :satisfied: (same emoji)

int get _totalEmojis =>
    data.emojisByCategory.fold(0, (sum, category) => sum + category.$2.length);

void main() {
  group('EmojiRepository.search', () {
    test('returns the matched shortcode alongside the emoji', () {
      final results = EmojiRepository.search('grinning');
      expect(
        results,
        contains(
          isA<EmojiSearchResult>()
              .having((r) => r.matchedShortcode, 'matchedShortcode', 'grinning')
              .having((r) => r.entry.emoji, 'emoji', _grinning),
        ),
      );
    });

    test('every matched shortcode contains the query', () {
      final results = EmojiRepository.search('smil');
      expect(results, isNotEmpty);
      expect(results.every((r) => r.matchedShortcode.contains('smil')), isTrue);
    });

    test('is case-insensitive', () {
      String codes(List<EmojiSearchResult> r) =>
          r.map((e) => e.matchedShortcode).join(',');
      expect(
        codes(EmojiRepository.search('GRIN')),
        codes(EmojiRepository.search('grin')),
      );
    });

    test('dedupes to one result per emoji', () {
      final results = EmojiRepository.search('a', limit: 1000);
      final glyphs = results.map((r) => r.entry.emoji).toList();
      expect(glyphs.toSet().length, glyphs.length);
    });

    test(
      'collapses an emoji with several matching shortcodes to one entry',
      () {
        // Query matches both `laughing` and `satisfied` (same emoji).
        final matches = EmojiRepository.search(
          'satisf',
        ).where((r) => r.entry.emoji == _laughing);
        expect(matches.length, 1);
      },
    );

    test('respects the limit', () {
      expect(
        EmojiRepository.search('a', limit: 5).length,
        lessThanOrEqualTo(5),
      );
    });

    test('sorts results by matched shortcode', () {
      final codes = EmojiRepository.search(
        'face',
        limit: 1000,
      ).map((r) => r.matchedShortcode).toList();
      final sorted = [...codes]..sort();
      expect(codes, sorted);
    });

    test('empty query returns the first emojis in canonical order', () {
      final results = EmojiRepository.search('', limit: 3);
      expect(results.length, 3);
      expect(results.first.entry.emoji, _grinning);
    });
  });

  group('EmojiRepository.filter', () {
    // Flattens the category-grouped result to a flat list of glyphs.
    List<String> glyphs(List<(String, List<data.Emoji>)> groups) =>
        groups.expand((group) => group.$2).map((e) => e.emoji).toList();

    test('empty query returns every emoji, grouped by category', () {
      final groups = EmojiRepository.filter('');
      expect(groups.length, data.emojisByCategory.length);
      expect(glyphs(groups).length, _totalEmojis);
    });

    test('empty query includes a known emoji', () {
      expect(glyphs(EmojiRepository.filter('')), contains(_grinning));
    });

    test('filters to emojis whose shortcode matches the query', () {
      expect(glyphs(EmojiRepository.filter('grinning')), contains(_grinning));
    });

    test('drops categories with no matches', () {
      final groups = EmojiRepository.filter('grinning');
      expect(groups.every((group) => group.$2.isNotEmpty), isTrue);
      expect(groups.length, lessThan(data.emojisByCategory.length));
    });

    test('is case-insensitive', () {
      expect(
        glyphs(EmojiRepository.filter('GRIN')),
        glyphs(EmojiRepository.filter('grin')),
      );
    });

    test('does not repeat an emoji that matches on several shortcodes', () {
      final all = glyphs(EmojiRepository.filter('a'));
      expect(all.toSet().length, all.length);
    });
  });
}
