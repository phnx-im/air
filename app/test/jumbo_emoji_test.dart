// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/core/api/markdown.dart';
import 'package:air/core/api/message_content.dart';
import 'package:air/message_list/jumbo_emoji.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:uuid/uuid.dart';

List<RangedInlineElement> _textInlines(String text) {
  return [
    RangedInlineElement(
      start: 0,
      end: text.length,
      element: InlineElement.text(text),
    ),
  ];
}

void main() {
  group('isJumboEmoji', () {
    test('single emoji returns true', () {
      expect(isJumboEmoji(_textInlines('😀')), isTrue);
    });

    test('3 emoji with spaces returns true', () {
      expect(isJumboEmoji(_textInlines('😀 😃 😄')), isTrue);
    });

    test('5 emoji returns true', () {
      expect(isJumboEmoji(_textInlines('😀😃😄😁😆')), isTrue);
    });

    test('6 emoji returns false', () {
      expect(isJumboEmoji(_textInlines('😀😃😄😁😆😅')), isFalse);
    });

    test('emoji + text returns false', () {
      expect(isJumboEmoji(_textInlines('😀 hello')), isFalse);
    });

    test('emoji + punctuation returns false', () {
      expect(isJumboEmoji(_textInlines('😀!')), isFalse);
    });

    test('ZWJ sequence counts as 1 token', () {
      // Family emoji (man + ZWJ + woman + ZWJ + girl)
      expect(isJumboEmoji(_textInlines('👨‍👩‍👧')), isTrue);
    });

    test('flag emoji counts as 1 token', () {
      // US flag
      expect(isJumboEmoji(_textInlines('🇺🇸')), isTrue);
    });

    test('empty string returns false', () {
      expect(isJumboEmoji(_textInlines('')), isFalse);
    });

    test('whitespace only returns false', () {
      expect(isJumboEmoji(_textInlines('   ')), isFalse);
    });

    test('non-text inline element returns false', () {
      final inlines = [
        const RangedInlineElement(
          start: 0,
          end: 1,
          element: InlineElement.code('😀'),
        ),
      ];
      expect(isJumboEmoji(inlines), isFalse);
    });

    test('skin tone modifier counts as part of emoji', () {
      expect(isJumboEmoji(_textInlines('👋🏽')), isTrue);
    });

    test('emoji with variation selector (❤️ U+2764 + U+FE0F) returns true', () {
      expect(isJumboEmoji(_textInlines('❤️')), isTrue);
    });

    test('keycap sequence (1️⃣) returns true', () {
      expect(isJumboEmoji(_textInlines('1️⃣')), isTrue);
    });

    test('subdivision flag (🏴󠁧󠁢󠁥󠁮󠁧󠁿 England) returns true', () {
      expect(isJumboEmoji(_textInlines('🏴󠁧󠁢󠁥󠁮󠁧󠁿')), isTrue);
    });
  });

  group('isJumboEmojiMessage', () {
    final topicId = Uint8List(16);

    UiMimiContent emojiMessage(String emoji) {
      return UiMimiContent(
        topicId: topicId,
        attachments: [],
        content: MessageContent(
          elements: [
            RangedBlockElement(
              start: 0,
              end: emoji.length,
              element: BlockElement.paragraph(_textInlines(emoji)),
            ),
          ],
        ),
      );
    }

    test('single emoji paragraph returns true', () {
      expect(isJumboEmojiMessage(emojiMessage('😀')), isTrue);
    });

    test('message with attachment returns false', () {
      final msg = UiMimiContent(
        topicId: topicId,
        attachments: [
          const UiAttachment(
            attachmentId: AttachmentId(
              uuid: UuidValue.raw('00000000-0000-0000-0000-000000000000'),
            ),
            filename: 'photo.png',
            contentType: 'image/png',
            size: 1024,
          ),
        ],
        content: MessageContent(
          elements: [
            RangedBlockElement(
              start: 0,
              end: 2,
              element: BlockElement.paragraph(_textInlines('😀')),
            ),
          ],
        ),
      );
      expect(isJumboEmojiMessage(msg), isFalse);
    });

    test('message with null content returns false', () {
      final msg = UiMimiContent(topicId: topicId, attachments: []);
      expect(isJumboEmojiMessage(msg), isFalse);
    });

    test('message with multiple block elements returns false', () {
      final msg = UiMimiContent(
        topicId: topicId,
        attachments: [],
        content: MessageContent(
          elements: [
            RangedBlockElement(
              start: 0,
              end: 2,
              element: BlockElement.paragraph(_textInlines('😀')),
            ),
            RangedBlockElement(
              start: 3,
              end: 5,
              element: BlockElement.paragraph(_textInlines('😃')),
            ),
          ],
        ),
      );
      expect(isJumboEmojiMessage(msg), isFalse);
    });

    test('message with non-paragraph block returns false', () {
      final msg = UiMimiContent(
        topicId: topicId,
        attachments: [],
        content: MessageContent(
          elements: [
            RangedBlockElement(
              start: 0,
              end: 2,
              element: BlockElement.heading(_textInlines('😀')),
            ),
          ],
        ),
      );
      expect(isJumboEmojiMessage(msg), isFalse);
    });

    test('message with text paragraph returns false', () {
      expect(isJumboEmojiMessage(emojiMessage('hello world')), isFalse);
    });
  });
}
