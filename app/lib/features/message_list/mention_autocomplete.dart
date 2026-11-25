// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/mention_picker/mention_picker.dart';
import 'package:air/ds/patterns/mention_picker/mention_picker_tokens.dart';
import 'package:air/features/message_list/widgets/suggestion_overlay.dart';
import 'package:air/features/message_list/widgets/text_autocomplete.dart';
import 'package:air/features/user/avatar.dart';
import 'package:flutter/material.dart';

/// Autocompletes `@` mentions of chat members in the composer.
///
/// The selected member is inserted as a markdown link to their mimi user URI
/// (`[@Display Name](mimi://<domain>/u/<uuid>)`) followed by a space, which the
/// message renderer shows as a mention pill.
class MentionAutocompleteStrategy
    implements TextAutocompleteStrategy<UiUserProfile> {
  MentionAutocompleteStrategy({
    required this.getMembers,
    required this.getProfile,
    required this.getOwnUserId,
    required this.isEnabled,
    this.mentionUri = _defaultMentionUri,
  });

  static const int suggestionLimit = 8;

  /// Returns the members of the current chat.
  final List<UiUserId> Function() getMembers;

  /// Resolves the profile of a member.
  final UiUserProfile Function(UiUserId userId) getProfile;

  /// Returns the local user's id (excluded from suggestions).
  final UiUserId Function() getOwnUserId;

  /// Whether mentions are on. While off, an `@` opens no picker.
  final bool Function() isEnabled;

  /// Formats the mention link destination; injectable for tests.
  final String Function(UiUserId userId) mentionUri;

  static String _defaultMentionUri(UiUserId userId) =>
      userMentionUri(userId: userId);

  /// Returns a trigger when the caret sits after an `@` mention query.
  @override
  AutocompleteTrigger? findTrigger(TextEditingValue value) {
    if (!isEnabled()) {
      return null;
    }
    // Only operate when the caret is collapsed and inside the text.
    if (!value.selection.isValid || !value.selection.isCollapsed) {
      return null;
    }
    final caret = value.selection.baseOffset;
    if (caret <= 0 || caret > value.text.length) {
      return null;
    }
    // Only trigger on an `@` immediately left of the caret with no whitespace
    // in between. The `@` must sit at the start of the text or be preceded by
    // whitespace, so word-interior `@`s (e.g. email addresses) don't match.
    final untilCaret = value.text.substring(0, caret);
    final match = RegExp(r'(^|\s)(@[^\s@]*)$').firstMatch(untilCaret);
    final fragment = match?.group(2);
    if (fragment == null) {
      return null;
    }
    final start = match!.end - fragment.length;
    // An empty query is valid: a bare `@` lists all members.
    return AutocompleteTrigger(
      start: start,
      end: caret,
      query: fragment.substring(1).toLowerCase(),
    );
  }

  /// Filter chat members by display name, excluding the local user.
  @override
  FutureOr<List<UiUserProfile>> suggestionsFor(String query) {
    final ownUserId = getOwnUserId();
    final suggestions = getMembers()
        .where((userId) => userId != ownUserId)
        .map(getProfile)
        .where((profile) => profile.displayName.toLowerCase().contains(query))
        .toList();
    suggestions.sort(
      (a, b) =>
          a.displayName.toLowerCase().compareTo(b.displayName.toLowerCase()),
    );
    return suggestions.take(suggestionLimit).toList();
  }

  /// Replace the `@query` text with a markdown mention link and put the caret
  /// after the space that follows it.
  @override
  TextEditingValue applySuggestion(
    TextEditingValue value,
    AutocompleteTrigger trigger,
    UiUserProfile suggestion,
  ) {
    final uri = mentionUri(suggestion.userId);
    // Re-resolve the profile: it may have finished loading after the
    // suggestion was built.
    final name = _escapeLinkText(getProfile(suggestion.userId).displayName);
    final mention = '[@$name]($uri)';
    // A mention picked in front of existing whitespace reuses it instead of
    // adding a second space.
    final followedBySpace = value.text
        .substring(trigger.end)
        .startsWith(RegExp(r'\s'));
    final newText = value.text.replaceRange(
      trigger.start,
      trigger.end,
      followedBySpace ? mention : '$mention ',
    );
    final newSelection = TextSelection.collapsed(
      offset: trigger.start + mention.length + 1,
    );
    return TextEditingValue(text: newText, selection: newSelection);
  }

  /// Style the overlay as the mention picker card.
  @override
  SuggestionOverlayStyle overlayStyle(BuildContext context) {
    return SuggestionOverlayStyle(
      backgroundColor: MentionPickerTokens.background(
        SemanticPalette.of(context),
      ),
      borderRadius: BorderRadius.circular(MentionPickerTokens.radius),
      elevation: MentionPickerTokens.elevation,
      minWidth: MentionPickerTokens.width,
      maxWidth: MentionPickerTokens.width,
      // Shows every suggestion while there is room above the field. The
      // overlay caps the height to that room and scrolls beyond it.
      maxHeight: double.infinity,
      padding: MentionPickerTokens.padding,
      itemSpacing: MentionPickerTokens.rowGap,
    );
  }

  /// Render each member suggestion row with avatar and display name.
  @override
  Widget buildSuggestionItem(
    BuildContext context,
    UiUserProfile suggestion,
    bool isHighlighted,
  ) {
    final tokens = MentionPickerTokens.current;
    return MentionPickerRow(
      tokens: tokens,
      avatar: UserAvatar(profile: suggestion, size: tokens.avatarSize),
      name: suggestion.displayName,
      highlighted: isHighlighted,
    );
  }

  /// Escapes characters that would break out of a markdown link text.
  String _escapeLinkText(String text) =>
      text.replaceAllMapped(RegExp(r'[\\\[\]]'), (match) => '\\${match[0]}');
}
