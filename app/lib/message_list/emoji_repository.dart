// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/message_list/emoji_data_generated.dart' as data;

enum EmojiSkinVariation {
  none(''),
  light('\u{1F3FB}'),
  mediumLight('\u{1F3FC}'),
  medium('\u{1F3FD}'),
  mediumDark('\u{1F3FE}'),
  dark('\u{1F3FF}'),
  redHair('\u{1F9B0}'),
  curlyHair('\u{1F9B1}'),
  bald('\u{1F9B2}'),
  whiteHair('\u{1F9B3}');

  const EmojiSkinVariation(this.modifier);

  /// The Unicode skintone modifier appended to a skinnable base emoji.
  final String modifier;
}

extension EmojiExtension on data.Emoji {
  /// Applies [variation] to [entry] using its precomputed skin-tone variant, falling
  /// back to the base emoji when the tone is [EmojiSkinVariation.none] or the variant
  /// is missing. Using the variant table (rather than appending the modifier)
  /// keeps ZWJ and multi-code-point emojis correct.
  String applySkinVariation(EmojiSkinVariation variation) {
    if (variation == .none) {
      return emoji;
    }
    return skinVariations[variation.modifier] ?? emoji;
  }
}

class EmojiRepository {
  /// All entries whose shortcodes contain [query] (case-insensitive). Returns
  /// the full set when [query] is empty. Unlike [search], this is unbounded and
  /// intended to back the emoji picker grid.
  static List<(String, List<data.Emoji>)> filter(String query) {
    final normalized = query.trim().toLowerCase();
    if (normalized.isEmpty) {
      return data.emojisByCategory;
    }
    final matches = data.shortcodeToIndex.entries
        .where((e) => e.key.contains(normalized))
        .map((e) => e.value)
        .toSet();

    return data.emojisByCategory.indexed
        .map((entry) {
          final (catId, (category, emojis)) = entry;
          return (
            category,
            emojis.indexed
                .where((e) => matches.contains((catId, e.$1)))
                .map((e) => e.$2)
                .toList(),
          );
        })
        .where((category) => category.$2.isNotEmpty)
        .toList();
  }

  /// Up to [limit] emojis whose shortcode contains [query] (case-insensitive),
  /// each paired with the matched shortcode. Results are deduped to one entry
  /// per emoji (keeping its primary/first matching shortcode). An empty [query]
  /// returns the first [limit] emojis in canonical order.
  static List<EmojiSearchResult> search(String query, {int limit = 20}) {
    final normalized = query.toLowerCase();
    final seen = <(int, int)>{};
    final results = <EmojiSearchResult>[];
    // `shortcodeToIndex` is ordered by emoji, primary shortcode first, so the
    // first surviving entry per emoji carries its best-matching shortcode.
    for (final entry in data.shortcodeToIndex.entries) {
      if (normalized.isNotEmpty && !entry.key.contains(normalized)) {
        continue;
      }
      if (!seen.add(entry.value)) {
        continue;
      }
      final (catId, index) = entry.value;
      results.add(
        EmojiSearchResult(
          entry: data.emojisByCategory[catId].$2[index],
          matchedShortcode: entry.key,
        ),
      );
      // Empty query keeps canonical order, so we can stop once full.
      if (normalized.isEmpty && results.length >= limit) {
        break;
      }
    }

    if (normalized.isNotEmpty) {
      results.sort((a, b) => a.matchedShortcode.compareTo(b.matchedShortcode));
    }

    return results.length > limit ? results.sublist(0, limit) : results;
  }

  static data.Emoji? byShortcode(String shortcode) {
    final emojiRef = data.shortcodeToIndex[shortcode.toLowerCase()];
    if (emojiRef == null) {
      return null;
    }

    final (category, emojis) = data.emojisByCategory[emojiRef.$1];
    return emojis[emojiRef.$2];
  }
}

class EmojiSearchResult {
  const EmojiSearchResult({
    required this.entry,
    required this.matchedShortcode,
  });

  final data.Emoji entry;
  final String matchedShortcode;
}
