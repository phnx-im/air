// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/emojis/generated.dart';
import 'package:air/message_list/emoji_repository.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/message_list/widgets/suggestion_overlay.dart';
import 'package:air/message_list/widgets/text_autocomplete.dart';
import 'package:flutter/material.dart';

class EmojiAutocompleteStrategy implements TextAutocompleteStrategy<Emoji> {
  static const int suggestionLimit = 5;
  final Map<Emoji, String> _displayShortcodes = {};

  /// Returns a trigger when the caret sits after a valid colon shortcode.
  @override
  AutocompleteTrigger? findTrigger(TextEditingValue value) {
    // Only operate when the caret is collapsed and inside the text.
    if (!value.selection.isValid || !value.selection.isCollapsed) {
      return null;
    }
    final caret = value.selection.baseOffset;
    if (caret <= 0 || caret > value.text.length) {
      return null;
    }
    // Search back from the caret for the most recent colon.
    final untilCaret = value.text.substring(0, caret);
    final match = RegExp(r':[A-Za-z0-9_\-\+]*:?$').firstMatch(untilCaret);
    if (match == null || match.start == match.end) {
      return null;
    }
    final start = match.start;
    final fragment = untilCaret.substring(match.start + 1);
    final trimmed = fragment.endsWith(':')
        ? fragment.substring(0, fragment.length - 1)
        : fragment;
    if (trimmed.isEmpty) {
      return null;
    }
    if (!_isValidQuery(trimmed)) {
      return null;
    }
    return AutocompleteTrigger(
      start: start,
      end: caret,
      query: trimmed.toLowerCase(),
    );
  }

  /// Fetch suggestions for a shortcode from the emoji repository.
  @override
  FutureOr<List<Emoji>> suggestionsFor(String query) async {
    final results = EmojiRepository.search(query, limit: suggestionLimit);
    _displayShortcodes
      ..clear()
      ..addEntries(
        results.map(
          (result) => MapEntry(result.entry, result.matchedShortcode),
        ),
      );
    return results.map((result) => result.entry).toList();
  }

  /// Replace the trigger text with the selected emoji character.
  @override
  TextEditingValue applySuggestion(
    TextEditingValue value,
    AutocompleteTrigger trigger,
    Emoji suggestion,
  ) {
    final newText = value.text.replaceRange(
      trigger.start,
      trigger.end,
      suggestion.emoji,
    );
    final newSelection = TextSelection.collapsed(
      offset: trigger.start + suggestion.emoji.length,
    );
    return TextEditingValue(text: newText, selection: newSelection);
  }

  /// Provide overlay styling consistent with the chat theme.
  @override
  SuggestionOverlayStyle overlayStyle(BuildContext context) {
    return SuggestionOverlayStyle(
      backgroundColor: CustomColorScheme.of(context).backgroundElevated.primary,
      borderRadius: BorderRadius.circular(Spacing.px16),
      elevation: 8,
      maxWidth: 320,
    );
  }

  /// Render each emoji suggestion row with the glyph and shortcode.
  @override
  Widget buildSuggestionItem(
    BuildContext context,
    Emoji suggestion,
    bool isHighlighted,
  ) {
    final scheme = CustomColorScheme.of(context);
    final backgroundColor = isHighlighted
        ? scheme.fill.primary
        : scheme.backgroundElevated.primary;
    return Container(
      color: backgroundColor,
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.px16,
        vertical: Spacing.px8,
      ),
      child: Row(
        children: [
          Text(
            suggestion.emoji,
            style: TextStyle(fontSize: BodyFontSize.large1.size),
          ),
          const SizedBox(width: Spacing.px8),
          Expanded(
            child: Text(
              ':${_displayShortcodes[suggestion]}:',
              style: TextStyle(
                fontSize: BodyFontSize.base.size,
                color: scheme.text.primary,
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// Validates the shortcode fragment against allowed characters.
  bool _isValidQuery(String query) {
    if (query.length > 40) {
      return false;
    }
    return RegExp(r'^[a-zA-Z0-9_\-\+]+$').hasMatch(query);
  }

  @override
  bool shouldCommitImmediately(
    TextEditingValue value,
    AutocompleteTrigger trigger,
  ) {
    if (trigger.end <= trigger.start || trigger.end > value.text.length) {
      return false;
    }
    final closingChar = value.text[trigger.end - 1];
    final openingChar = value.text[trigger.start];
    return closingChar == ':' && openingChar == ':' && trigger.query.isNotEmpty;
  }

  @override
  bool matchesQuery(Emoji suggestion, String query) {
    return EmojiRepository.byShortcode(query)?.emoji == suggestion.emoji;
  }

  @override
  FutureOr<Emoji?> directMatch(String query) async {
    return EmojiRepository.byShortcode(query);
  }
}
