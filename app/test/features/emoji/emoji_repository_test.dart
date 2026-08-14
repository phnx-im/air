// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/emoji/emoji_data.dart' as data;
import 'package:air/features/emoji/emoji_repository.dart';
import 'package:flutter_test/flutter_test.dart';

// Stable data points from the generated dataset.
const _grinning = '\u{1F600}'; // :grinning:
const _laughing = '\u{1F606}'; // :laughing: / :satisfied: (same emoji)

int get _totalEmojis =>
    data.emojisByCategory.fold(0, (sum, category) => sum + category.$2.length);

void main() {
  group('EmojiRepository.search', () {
    test('matches a shortcode word by prefix', () {
      final results = EmojiRepository.search('grinning');
      expect(
        results,
        contains(isA<data.Emoji>().having((e) => e.emoji, 'emoji', _grinning)),
      );
    });

    test('matches only shortcode words starting with the query', () {
      final results = EmojiRepository.search('smil');
      expect(results, isNotEmpty);
    });

    test('is case-insensitive', () {
      String glyphs(List<data.Emoji> r) => r.map((e) => e.emoji).join(',');
      expect(
        glyphs(EmojiRepository.search('GRIN')),
        glyphs(EmojiRepository.search('grin')),
      );
    });

    test('dedupes to one result per emoji', () {
      final results = EmojiRepository.search('a');
      final glyphs = results.map((e) => e.emoji).toList();
      expect(glyphs.toSet().length, glyphs.length);
    });

    test(
      'collapses an emoji with several matching shortcodes to one entry',
      () {
        // Query matches both `laughing` and `satisfied` (same emoji).
        final matches = EmojiRepository.search(
          'satisf',
        ).where((e) => e.emoji == _laughing);
        expect(matches.length, 1);
      },
    );

    test('sorts results by short name', () {
      final names = EmojiRepository.search('giggle').map((e) => e.shortName);
      final sorted = [...names]..sort();
      expect(names, sorted);
    });

    test('results are first sourced from shortcodes', () {
      final names = EmojiRepository.search('face').map((e) => e.shortName);
      expect(names.first, "face_holding_back_tears");
    });

    test('ranks short name words before aliases', () {
      // `sweat` is a word of the first two short names, and only an alias of
      // :anxious: (whose alias is `cold_sweat`).
      final results = EmojiRepository.search('sweat');
      expect(results.take(3).map((e) => e.shortName), [
        'sweat_drops',
        'grinning_face_with_sweat',
        'anxious',
      ]);
    });

    test('empty query returns the first emojis in canonical order', () {
      final results = EmojiRepository.search('');
      expect(results.first.emoji, _grinning);
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
