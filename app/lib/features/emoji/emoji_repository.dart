// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/emoji/emoji_data.dart' as data;

enum EmojiSkinVariation {
  none(''),
  light('\u{1F3FB}'),
  mediumLight('\u{1F3FC}'),
  medium('\u{1F3FD}'),
  mediumDark('\u{1F3FE}'),
  dark('\u{1F3FF}');

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
  /// All entries whose shortcode words start with [query] (case-insensitive).
  /// Returns the full set when [query] is empty. Unlike [search], this is
  /// unbounded and intended to back the emoji picker grid.
  static List<(String, List<data.Emoji>)> filter(String query) {
    final normalized = query.trim().toLowerCase();
    if (normalized.isEmpty) {
      return data.emojisByCategory;
    }
    final matches = data.tagsToIndex.entries
        .where((e) => e.key.startsWith(normalized))
        .expand((e) => e.value)
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

  /// Up to [limit] emojis whose shortcode words start with [query]
  /// (case-insensitive), deduped to one entry per emoji. Emojis matching on a
  /// word of their own short name come first, ranked by the position of that
  /// word, then by short name. Emojis matching only through an alias or tag
  /// follow. An empty [query] returns the first [limit] emojis
  /// in canonical order.
  static List<data.Emoji> search(String query, {int limit = 50}) {
    final normalized = query.toLowerCase();
    if (normalized.isEmpty) {
      return data.emojisByCategory
          .expand((category) => category.$2)
          .take(limit)
          .toList();
    }

    final seen = <data.EmojiRef>{};
    final byShortName = <(int, data.Emoji)>[];
    final byAlias = <data.Emoji>[];
    for (final entry in data.tagsToIndex.entries) {
      if (!entry.key.startsWith(normalized)) {
        continue;
      }
      for (final ref in entry.value) {
        if (!seen.add(ref)) {
          continue;
        }
        final emoji = data.emojisByCategory[ref.$1].$2[ref.$2];
        final position = emoji.shortName
            .split('_')
            .indexWhere((word) => word.startsWith(normalized));
        if (position < 0) {
          byAlias.add(emoji);
        } else {
          byShortName.add((position, emoji));
        }
      }
    }

    byShortName.sort((a, b) {
      final byPosition = a.$1.compareTo(b.$1);
      return byPosition != 0
          ? byPosition
          : a.$2.shortName.compareTo(b.$2.shortName);
    });
    byAlias.sort((a, b) => a.shortName.compareTo(b.shortName));

    final matching = byShortName.map((match) => match.$2).take(limit).toList();
    matching.addAll(byAlias.take(limit - matching.length));
    return matching;
  }
}
