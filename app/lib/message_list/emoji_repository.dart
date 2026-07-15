// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:collection';

import 'package:air/emojis/generated.dart' as data;

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
  /// (case-insensitive). Results are deduped to one entry per emoji. An empty
  /// [query] returns the first [limit] emojis in canonical order.
  static List<data.Emoji> search(String query, {int limit = 20}) {
    final normalized = query.toLowerCase();
    if (normalized.isEmpty) {
      return [];
    }

    final seen = <(int, int)>{};
    final matchingShortcodes = SplayTreeSet<data.Emoji>(
      (a, b) => a.shortName.compareTo(b.shortName),
    );
    for (final (catId, category) in data.emojisByCategory.indexed) {
      for (final (index, emoji) in category.$2.indexed) {
        if (!emoji.shortName.startsWith(normalized)) {
          continue;
        }
        if (!seen.add((catId, index))) {
          continue;
        }

        matchingShortcodes.add(emoji);

        if (matchingShortcodes.length >= limit) {
          return matchingShortcodes.toList();
        }
      }
    }

    final matchingTags = SplayTreeSet<data.Emoji>(
      (a, b) => a.shortName.compareTo(b.shortName),
    );
    for (final entry in data.tagsToIndex.entries) {
      if (!entry.key.startsWith(normalized)) {
        continue;
      }
      for (final ref in entry.value) {
        if (!seen.add(ref)) {
          continue;
        }
        final (catId, index) = ref;
        matchingTags.add(data.emojisByCategory[catId].$2[index]);
      }

      if ((matchingShortcodes.length + matchingTags.length) >= limit) {
        break;
      }
    }

    return matchingShortcodes.toList() +
        matchingTags.take(limit - matchingShortcodes.length).toList();
  }
}
