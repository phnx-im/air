// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/message_list/mention_autocomplete.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  final alice = 1.userId();
  final bob = 2.userId();
  final carol = 3.userId();

  UiUserProfile profile(UiUserId userId, String displayName) =>
      UiUserProfile(userId: userId, displayName: displayName);

  final profiles = {
    alice: profile(alice, 'Alice'),
    bob: profile(bob, 'Bob Smith'),
    carol: profile(carol, 'Carol'),
  };

  String testUri(UiUserId userId) => 'mimi://${userId.domain}/u/${userId.uuid}';

  MentionAutocompleteStrategy buildStrategy({bool enabled = true}) =>
      MentionAutocompleteStrategy(
        getMembers: () => [alice, bob, carol],
        getProfile: (userId) => profiles[userId]!,
        getOwnUserId: () => alice,
        isEnabled: () => enabled,
        mentionUri: testUri,
      );

  TextEditingValue value(String text, [int? caret]) => TextEditingValue(
    text: text,
    selection: TextSelection.collapsed(offset: caret ?? text.length),
  );

  group('MentionAutocompleteStrategy', () {
    group('findTrigger', () {
      test('triggers on @ at start of input', () {
        final trigger = buildStrategy().findTrigger(value('@bo'));
        expect(trigger, isNotNull);
        expect(trigger!.start, 0);
        expect(trigger.end, 3);
        expect(trigger.query, 'bo');
      });

      test('never triggers while mentions are off', () {
        expect(buildStrategy(enabled: false).findTrigger(value('@bo')), isNull);
      });

      test('triggers on bare @ with empty query', () {
        final trigger = buildStrategy().findTrigger(value('@'));
        expect(trigger, isNotNull);
        expect(trigger!.query, '');
      });

      test('triggers on @ after whitespace', () {
        final trigger = buildStrategy().findTrigger(value('hello @Bo'));
        expect(trigger, isNotNull);
        expect(trigger!.start, 6);
        expect(trigger.query, 'bo');
      });

      test('does not trigger on word-interior @ (email addresses)', () {
        expect(buildStrategy().findTrigger(value('user@example.com')), isNull);
      });

      test('does not trigger when query contains whitespace', () {
        expect(buildStrategy().findTrigger(value('@bob smith')), isNull);
      });

      test('does not trigger when caret is before the query', () {
        expect(buildStrategy().findTrigger(value('hi @bob', 2)), isNull);
      });

      test('does not trigger on expanded selection', () {
        const expanded = TextEditingValue(
          text: '@bob',
          selection: TextSelection(baseOffset: 0, extentOffset: 4),
        );
        expect(buildStrategy().findTrigger(expanded), isNull);
      });
    });

    group('suggestionsFor', () {
      test('filters case-insensitively and sorts alphabetically', () async {
        final suggestions = await buildStrategy().suggestionsFor('o');
        expect(suggestions.map((s) => s.displayName), ['Bob Smith', 'Carol']);
      });

      test('excludes the local user', () async {
        final suggestions = await buildStrategy().suggestionsFor('');
        expect(suggestions.map((s) => s.userId), isNot(contains(alice)));
        expect(suggestions, hasLength(2));
      });

      test('matches anywhere in the display name', () async {
        final suggestions = await buildStrategy().suggestionsFor('smith');
        expect(suggestions.map((s) => s.displayName), ['Bob Smith']);
      });
    });

    group('applySuggestion', () {
      test('replaces the trigger with a mention link and a space', () {
        final strategy = buildStrategy();
        final trigger = strategy.findTrigger(value('hello @bo'))!;
        final suggestion = profiles[bob]!;

        final result = strategy.applySuggestion(
          value('hello @bo'),
          trigger,
          suggestion,
        );

        expect(result.text, 'hello [@Bob Smith](${testUri(bob)}) ');
        expect(result.selection.baseOffset, result.text.length);
      });

      test('reuses the whitespace already after the trigger', () {
        final strategy = buildStrategy();
        final before = value('@bo and more', 3);
        final trigger = strategy.findTrigger(before)!;

        final result = strategy.applySuggestion(
          before,
          trigger,
          profiles[bob]!,
        );

        final mention = '[@Bob Smith](${testUri(bob)})';
        expect(result.text, '$mention and more');
        expect(result.selection.baseOffset, mention.length + 1);
      });

      test('re-resolves the display name at apply time', () {
        // The profile may finish loading between building the suggestion
        // (placeholder name) and applying it.
        var name = 'Sneaky Tapir';
        final strategy = MentionAutocompleteStrategy(
          getMembers: () => [bob],
          getProfile: (_) => profile(bob, name),
          getOwnUserId: () => alice,
          isEnabled: () => true,
          mentionUri: testUri,
        );
        final trigger = strategy.findTrigger(value('@sn'))!;
        final suggestion = profile(bob, name);
        name = 'Bob Smith';

        final result = strategy.applySuggestion(
          value('@sn'),
          trigger,
          suggestion,
        );

        expect(result.text, '[@Bob Smith](${testUri(bob)}) ');
      });

      test('escapes markdown link text characters in display names', () {
        final tricky = profile(carol, r'Carol [x] \o/');
        final strategy = MentionAutocompleteStrategy(
          getMembers: () => [carol],
          getProfile: (_) => tricky,
          getOwnUserId: () => alice,
          isEnabled: () => true,
          mentionUri: testUri,
        );
        final trigger = strategy.findTrigger(value('@ca'))!;

        final result = strategy.applySuggestion(value('@ca'), trigger, tricky);

        expect(
          result.text,
          r'[@Carol \[x\] \\o/]'
          '(${testUri(carol)}) ',
        );
      });
    });
  });
}
