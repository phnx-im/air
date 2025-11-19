// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:convert';

import 'package:flutter/services.dart' show rootBundle;

class EmojiEntry {
  const EmojiEntry({
    required this.shortcodes,
    required this.emoji,
    required this.supportsSkinTone,
  });

  factory EmojiEntry.fromJson(Map<String, dynamic> json) {
    return EmojiEntry(
      shortcodes: (json['s'] as List<dynamic>)
          .map((value) => (value as String).toLowerCase())
          .toList(),
      emoji: json['e'] as String,
      supportsSkinTone: (json['t'] as num?) == 1,
    );
  }

  final List<String> shortcodes;
  final String emoji;
  final bool supportsSkinTone;
}

class EmojiRepository {
  EmojiRepository._(this._entries, this._index);

  final List<EmojiEntry> _entries;
  final Map<String, EmojiEntry> _index;

  static Future<EmojiRepository> load() async {
    final raw = await rootBundle.loadString('assets/emoji/emoji.json');
    final parsed = (jsonDecode(raw) as List<dynamic>)
        .cast<Map<String, dynamic>>()
        .map(EmojiEntry.fromJson)
        .toList();
    final index = <String, EmojiEntry>{};
    for (final entry in parsed) {
      for (final shortcode in entry.shortcodes) {
        index[shortcode] = entry;
      }
    }
    return EmojiRepository._(parsed, index);
  }

  List<EmojiEntry> top({int limit = 10}) {
    return _entries.take(limit).toList();
  }

  List<EmojiSearchResult> search(String query, {int limit = 20}) {
    final normalized = query.toLowerCase();
    if (normalized.isEmpty) {
      return top(limit: limit)
          .map(
            (entry) => EmojiSearchResult(
              entry: entry,
              matchedShortcode: entry.shortcodes.first,
            ),
          )
          .toList();
    }

    final List<EmojiSearchResult> results = [];
    for (final entry in _entries) {
      final match = entry.shortcodes.firstWhere(
        (code) => code.contains(normalized),
        orElse: () => '',
      );
      if (match.isEmpty) {
        continue;
      }
      results.add(EmojiSearchResult(entry: entry, matchedShortcode: match));
    }

    results.sort((a, b) => a.matchedShortcode.compareTo(b.matchedShortcode));

    if (results.length > limit) {
      return results.sublist(0, limit);
    }
    return results;
  }

  EmojiEntry? byShortcode(String shortcode) {
    return _index[shortcode.toLowerCase()];
  }
}

class EmojiSearchResult {
  const EmojiSearchResult({
    required this.entry,
    required this.matchedShortcode,
  });

  final EmojiEntry entry;
  final String matchedShortcode;
}
