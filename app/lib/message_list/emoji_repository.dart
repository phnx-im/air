// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/message_list/emoji_data_generated.dart' as data;

enum EmojiSkinTone {
  none(''),
  light('\u{1F3FB}'),
  mediumLight('\u{1F3FC}'),
  medium('\u{1F3FD}'),
  mediumDark('\u{1F3FE}'),
  dark('\u{1F3FF}');

  const EmojiSkinTone(this.modifier);

  /// The Unicode skintone modifier appended to a skinnable base emoji.
  final String modifier;
}

class EmojiRepository {
  /// All entries whose shortcodes contain [query] (case-insensitive). Returns
  /// the full set when [query] is empty. Unlike [search], this is unbounded and
  /// intended to back the emoji picker grid.
  List<data.Emoji> filter(String query) {
    final normalized = query.trim().toLowerCase();
    if (normalized.isEmpty) {
      return data.emojisByCategory.expand((category) => category.$2).toList();
    }
    final matches = data.shortcodeToIndex.entries
        .where((e) => e.key.contains(normalized))
        .map((e) => e.value)
        .toList();

    return matches.expand((match) sync* {
      final (catId, index) = match;
      yield data.emojisByCategory[catId].$2[index];
    }).toList();
  }

  static List<EmojiSearchResult> search(String query, {int limit = 20}) {
    return [];
    // final normalized = query.toLowerCase();
    // if (normalized.isEmpty) {
    //   return top(limit: limit)
    //       .map(
    //         (entry) => EmojiSearchResult(
    //           entry: entry,
    //           matchedShortcode: entry.shortcodes.first,
    //         ),
    //       )
    //       .toList();
    // }

    // final List<EmojiSearchResult> results = [];
    // for (final entry in _entries) {
    //   final match = entry.shortcodes.firstWhere(
    //     (code) => code.contains(normalized),
    //     orElse: () => '',
    //   );
    //   if (match.isEmpty) {
    //     continue;
    //   }
    //   results.add(EmojiSearchResult(entry: entry, matchedShortcode: match));
    // }

    // results.sort((a, b) => a.matchedShortcode.compareTo(b.matchedShortcode));

    // if (results.length > limit) {
    //   return results.sublist(0, limit);
    // }
    // return results;
  }

  static data.Emoji? byShortcode(String shortcode) {
    final emojiRef = data.shortcodeToIndex[shortcode];
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
