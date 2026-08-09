// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/message_list/message_list_view.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/message_list/message_cubit.dart';
import 'package:air/ds/patterns/message_meta/message_meta.dart';
import 'package:air/ds/patterns/reaction_bar/reaction_bar.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter/gestures.dart';
import 'package:intl/intl.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

// NB: do not forget to adjust this, when you add more content to render
const highTestSize = Size(1080, 4500);

final chatId = 1.chatId();

final firstMessageContent = UiMimiContent(
  plainBody: 'Hello Alice from Bob',
  topicId: Uint8List(0),
  content: simpleMessage('Hello Alice from Bob'),
  attachments: [],
);

final firstDeletedMessageContent = UiMimiContent(
  topicId: Uint8List(0),
  attachments: [],
  replaces: Uint8List(0),
);

final veryLongMimiContent = UiMimiContent(
  topicId: Uint8List(0),
  plainBody: '''Nice to see you both here! 👋

This is a message with multiple lines. It should be properly displayed in the message bubble and split between multiple lines.''',
  content: simpleMessage(
    '''Nice to see you both here! 👋

This is a message with multiple lines. It should be properly displayed in the message bubble and split between multiple lines.''',
  ),
  attachments: [],
);

final messages = [
  UiChatMessage(
    id: 1.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'Hello Alice from Bob',
          topicId: Uint8List(0),
          content: simpleMessage('Hello Alice from Bob'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 2.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:01:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: true,
        content: UiMimiContent(
          plainBody:
              'Hello Alice. This is a long message that should not be truncated but properly split into multiple lines.',
          topicId: Uint8List(0),
          content: simpleMessage(
            'Hello Alice. This is a long message that should not be truncated but properly split into multiple lines.',
          ),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 100.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:01.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: false,
        content: richContent,
      ),
    ),
    status: UiMessageStatus.delivered,
    reactions: [],
  ),
  UiChatMessage(
    id: 3.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:02:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: true,
        content: UiMimiContent(
          plainBody: 'Hello Bob and Eve',
          topicId: Uint8List(0),
          content: simpleMessage('Hello Bob and Eve'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 4.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:03:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'How are you doing?',
          topicId: Uint8List(0),
          content: simpleMessage('How are you doing?'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 5.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:03:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: veryLongMimiContent,
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 7.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:08:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: richContent,
      ),
    ),
    status: UiMessageStatus.delivered,
    reactions: [],
  ),
  UiChatMessage(
    id: 8.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:13:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is a delivered message",
          content: simpleMessage("This is a delivered message"),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.delivered,
    reactions: [],
  ),
  UiChatMessage(
    id: 9.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:18:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is a read message",
          content: simpleMessage("This is a read message"),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.read,
    reactions: [],
  ),
  UiChatMessage(
    id: 10.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:23:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is a reply to Bob",
          content: simpleMessage("Hello Bob from Alice"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 1.messageId(),
      sender: 1.userId(),
      mimiContent: firstMessageContent,
    ),
    status: UiMessageStatus.read,
    reactions: [],
  ),
  UiChatMessage(
    id: 11.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:24:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: firstDeletedMessageContent,
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 12.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:28:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Bob, wrong chat",
          content: simpleMessage("Bob, wrong chat"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 11.messageId(),
      sender: 2.userId(),
      mimiContent: firstDeletedMessageContent,
    ),
    status: UiMessageStatus.read,
    reactions: [],
  ),
  UiChatMessage(
    id: 13.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:33:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is an answer to a message I deleted locally",
          content: simpleMessage(
            "This is an answer to a message I deleted locally",
          ),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: const UiInReplyToMessage.notFound(),
    status: UiMessageStatus.read,
    reactions: [],
  ),
  // Medium-sized message with three reactions (2+1)
  UiChatMessage(
    id: 14.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-02T00:00:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "I love this app!",
          content: simpleMessage("I love this app!"),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.read,
    reactions: [
      UiReaction(emoji: "🫪", users: [1.userId(), 3.userId()]),
      UiReaction(emoji: "💖", users: [4.userId()]),
    ],
  ),
  // Very short message with many reactions, to show the overflow pill
  UiChatMessage(
    id: 15.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-02T00:05:07.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "OK",
          content: simpleMessage("OK"),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.read,
    reactions: [
      UiReaction(emoji: "👍", users: [1.userId()]),
      UiReaction(emoji: "🤯", users: [5.userId()]),
      UiReaction(emoji: "🤨", users: [4.userId()]),
    ],
  ),
  // Own message with a long single line (including an unbreakable URL) that
  // must wrap inside the bubble. Guards against the bubble losing its width
  // constraint on desktop, where the hover react affordance wraps it in a Row.
  UiChatMessage(
    id: 16.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-02T00:06:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody:
              'Here is a really long single line without any hard breaks that '
              'has to wrap onto multiple lines inside the bubble instead of '
              'overflowing https://example.com/some/really/long/path/that/keeps/going',
          content: simpleMessage(
            'Here is a really long single line without any hard breaks that '
            'has to wrap onto multiple lines inside the bubble instead of '
            'overflowing https://example.com/some/really/long/path/that/keeps/going',
          ),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.read,
    reactions: [],
  ),
];

final richContent = UiMimiContent(
  topicId: Uint8List(0),
  plainBody: "This is a message with a link https://example.com",
  content: const MessageContent(
    elements: [
      RangedBlockElement(
        start: 0,
        end: 0,
        element: BlockElement_Paragraph([
          RangedInlineElement(
            start: 0,
            end: 0,
            element: InlineElement_Text("This is a rich content message "),
          ),
          RangedInlineElement(
            start: 0,
            end: 0,
            element: InlineElement_Link(
              destUrl: "https://example.com",
              children: [
                RangedInlineElement(
                  start: 0,
                  end: 0,
                  element: InlineElement_Text("https://example.com"),
                ),
              ],
            ),
          ),
        ]),
      ),
      RangedBlockElement(
        start: 0,
        end: 0,
        element: BlockElement_Quote([
          RangedBlockElement(
            start: 0,
            end: 0,
            element: BlockElement_Paragraph([
              RangedInlineElement(
                start: 0,
                end: 0,
                element: InlineElement_Text("This is a quote "),
              ),
              RangedInlineElement(
                start: 0,
                end: 0,
                element: InlineElement_Link(
                  destUrl: "https://example.com",
                  children: [
                    RangedInlineElement(
                      start: 0,
                      end: 0,
                      element: InlineElement_Text("https://example.com"),
                    ),
                  ],
                ),
              ),
            ]),
          ),
        ]),
      ),
    ],
  ),
  attachments: [],
);

/// A paragraph with an inline code span, followed by a fenced code block, so
/// one image carries monospace at both sizes next to the proportional prose.
final codeBlockContent = UiMimiContent(
  topicId: Uint8List(0),
  plainBody:
      'Call `main` first:\n\n```\nfn main() {\n'
      '    let n = 21;\n    println!("{n}");\n}\n```',
  content: const MessageContent(
    elements: [
      RangedBlockElement(
        start: 0,
        end: 0,
        element: BlockElement_Paragraph([
          RangedInlineElement(
            start: 0,
            end: 0,
            element: InlineElement_Text('Call '),
          ),
          RangedInlineElement(
            start: 0,
            end: 0,
            element: InlineElement_Code('main'),
          ),
          RangedInlineElement(
            start: 0,
            end: 0,
            element: InlineElement_Text(' first:'),
          ),
        ]),
      ),
      RangedBlockElement(
        start: 0,
        end: 0,
        element: BlockElement_CodeBlock([
          RangedCodeBlock(start: 0, end: 0, value: 'fn main() {'),
          RangedCodeBlock(start: 0, end: 0, value: '    let n = 21;'),
          RangedCodeBlock(start: 0, end: 0, value: '    println!("{n}");'),
          RangedCodeBlock(start: 0, end: 0, value: '}'),
        ]),
      ),
    ],
  ),
  attachments: [],
);

final codeBlockMessages = [
  // Plain prose, as the reference the monospace sits next to.
  UiChatMessage(
    id: 30.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:30:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'Have a look at the snippet below.',
          topicId: Uint8List(0),
          content: simpleMessage('Have a look at the snippet below.'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Incoming, so the slab carries `message.otherText`.
  UiChatMessage(
    id: 31.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:31:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: codeBlockContent,
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Outgoing, so the same slab carries `message.selfText`.
  UiChatMessage(
    id: 32.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:32:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: codeBlockContent,
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
];

final jumboEmojiMessages = [
  // Jumbo: single emoji from other user
  UiChatMessage(
    id: 20.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: '😀',
          topicId: Uint8List(0),
          content: simpleMessage('😀'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Jumbo: multiple emoji from self
  UiChatMessage(
    id: 21.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:11:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: '🎉🥳🎊',
          topicId: Uint8List(0),
          content: simpleMessage('🎉🥳🎊'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Not jumbo: emoji + text (should keep bubble)
  UiChatMessage(
    id: 22.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:12:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: '😀 hello',
          topicId: Uint8List(0),
          content: simpleMessage('😀 hello'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Not jumbo: edited emoji-only (should keep bubble)
  UiChatMessage(
    id: 23.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:13:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: true,
        content: UiMimiContent(
          plainBody: '👍',
          topicId: Uint8List(0),
          content: simpleMessage('👍'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Normal text message for contrast
  UiChatMessage(
    id: 24.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:19:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'Nice!',
          topicId: Uint8List(0),
          content: simpleMessage('Nice!'),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
];

final imageAttachment = UiAttachment(
  attachmentId: 2.attachmentId(),
  filename: "image.png",
  size: 10 * 1024 * 1024,
  contentType: 'image/png',
  description: "A woman eating a donut",
  imageMetadata: const UiImageMetadata(
    blurhash: "LEHLk~WB2yk8pyo0adR*.7kCMdnj",
    width: 100,
    height: 50,
  ),
);

final attachmentMessages = [
  UiChatMessage(
    id: 6.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "A File Attachment",
          content: simpleMessage('A File Attachment'),
          attachments: [
            UiAttachment(
              attachmentId: 1.attachmentId(),
              filename: "file.zip",
              contentType: "application/zip",
              size: 1024,
              description: "Failing golden tests",
            ),
          ],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 7.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:01.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Look what I've got to eat",
          content: simpleMessage("Look what I've got to eat"),
          attachments: [imageAttachment],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 8.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          attachments: [imageAttachment],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  UiChatMessage(
    id: 9.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:16:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Small image",
          content: simpleMessage("Small image"),
          attachments: [
            imageAttachment.copyWith(
              imageMetadata: imageAttachment.imageMetadata!.copyWith(
                width: 10,
                height: 10,
              ),
            ),
          ],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
];

final replyMessages = [
  // Long reply, short message
  UiChatMessage(
    id: 20.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Ok!",
          content: simpleMessage("Ok!"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 1.messageId(),
      sender: 3.userId(),
      mimiContent: veryLongMimiContent,
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Short reply, long message
  UiChatMessage(
    id: 21.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:01.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: veryLongMimiContent,
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 2.messageId(),
      sender: 2.userId(),
      mimiContent: UiMimiContent(
        topicId: Uint8List(0),
        plainBody: "Hi!",
        content: simpleMessage("Hi!"),
        attachments: [],
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Reply and a single emoji message
  UiChatMessage(
    id: 22.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:02.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "👍",
          content: simpleMessage("👍"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 1.messageId(),
      sender: 3.userId(),
      mimiContent: firstMessageContent,
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
  // Reply containing only emoji and some message
  UiChatMessage(
    id: 23.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:03.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "That was exactly my reaction!",
          content: simpleMessage("That was exactly my reaction!"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 3.messageId(),
      sender: 2.userId(),
      mimiContent: UiMimiContent(
        topicId: Uint8List(0),
        plainBody: "🎉🎊✨",
        content: simpleMessage("🎉🎊✨"),
        attachments: [],
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
  ),
];

MessageCubit createMockMessageCubit({
  required UserCubit userCubit,
  required MessageState initialState,
}) => MockMessageCubit(initialState: initialState);

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
    registerFallbackValue(0.attachmentId());
  });

  group('MessageListView', () {
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockAttachmentsRepository attachmentsRepository;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      attachmentsRepository = MockAttachmentsRepository();
      userSettingsCubit = MockUserSettingsCubit();

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: 'untilMessageId'),
          untilTimestamp: any(named: 'untilTimestamp'),
        ),
      ).thenAnswer((_) => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    });

    /// [platform] stands in for the host, which decides whether the rows carry
    /// the pointer affordances or the touch ones.
    Widget buildSubject({TargetPlatform? platform}) =>
        RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
          child: MultiBlocProvider(
            providers: [
              BlocProvider<UserCubit>.value(value: userCubit),
              BlocProvider<UsersCubit>.value(value: contactsCubit),
              BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
              BlocProvider<MessageListCubit>.value(value: messageListCubit),
              BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
            ],
            child: Builder(
              builder: (context) {
                return MaterialApp(
                  debugShowCheckedModeBanner: false,
                  theme: testThemeData(
                    MediaQuery.platformBrightnessOf(context),
                  ).copyWith(platform: platform),
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  home: const Scaffold(
                    body: MessageListView(
                      createMessageCubit: createMockMessageCubit,
                    ),
                  ),
                );
              },
            ),
          ),
        );

    group('message stamps', () {
      // The reader is user 1, per the MockUiUser above.
      UiChatMessage stampFixture(
        int id, {
        required int sender,
        bool edited = false,
        UiMessageStatus status = UiMessageStatus.delivered,
      }) => UiChatMessage(
        id: id.messageId(),
        chatId: chatId,
        timestamp: DateTime.parse('2023-01-01T00:0$id:00.000Z'),
        message: UiMessage_Content(
          UiContentMessage(
            sender: sender.userId(),
            sent: true,
            edited: edited,
            content: UiMimiContent(
              plainBody: 'Message $id',
              topicId: Uint8List(0),
              content: simpleMessage('Message $id'),
              attachments: [],
            ),
          ),
        ),
        status: status,
        reactions: [],
      );

      // The clock the stamp of message [id] carries. The fixtures are old
      // enough that no stamp is still on its relative tier.
      String stampLabel(int id) {
        const locale = 'en_US';
        return TimeFormats(
          locale: locale,
          timePattern: DateFormat.jm(locale).pattern!,
          datePattern: DateFormat.yMd(locale).pattern!,
        ).clock(DateTime.parse('2023-01-01T00:0$id:00.000Z').toLocal());
      }

      testWidgets('go on the end of the chat', (tester) async {
        // Oldest first, and nobody's own: every row is a group of its own, so
        // a rule keyed on the group boundaries rather than on the end of the
        // chat would stamp them all.
        messageListCubit.setState([
          stampFixture(1, sender: 2),
          stampFixture(2, sender: 3),
          stampFixture(3, sender: 2),
          stampFixture(4, sender: 3),
        ]);

        await tester.pumpWidget(buildSubject());

        expect(find.byType(MessageMeta), findsOneWidget);
        expect(find.text(stampLabel(4)), findsOneWidget);
      });

      testWidgets("go on the reader's own last message too", (tester) async {
        messageListCubit.setState([
          stampFixture(1, sender: 1),
          stampFixture(2, sender: 2),
          stampFixture(3, sender: 1),
          stampFixture(4, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject());

        // The reader's own last message keeps its stamp even though someone has
        // replied since. It reports how far it got rather than when it was
        // sent: the clock belongs to the end of the chat alone.
        expect(find.byType(MessageMeta), findsNWidgets(2));
        expect(find.text('Delivered'), findsOneWidget);
        expect(find.text(stampLabel(3)), findsNothing);
        expect(find.text(stampLabel(4)), findsOneWidget);
      });

      testWidgets('read as clock, glyph and word at the end of the chat', (
        tester,
      ) async {
        messageListCubit.setState([
          stampFixture(1, sender: 2),
          stampFixture(2, sender: 1, status: UiMessageStatus.read),
        ]);

        await tester.pumpWidget(buildSubject());

        // Own and newest at once, so the row carries all three: the reader's
        // own last word is also the end of the chat.
        expect(find.byType(MessageMeta), findsOneWidget);
        expect(find.text(stampLabel(2)), findsOneWidget);
        expect(find.text('Read'), findsOneWidget);
      });

      testWidgets('mark an edited message wherever it sits, without a time', (
        tester,
      ) async {
        messageListCubit.setState([
          stampFixture(1, sender: 2),
          stampFixture(2, sender: 3, edited: true),
          stampFixture(3, sender: 2),
          stampFixture(4, sender: 3),
        ]);

        await tester.pumpWidget(buildSubject());

        // The edited message, and the end of the chat.
        expect(find.byType(MessageMeta), findsNWidgets(2));
        // The marker is the reader's business; when the edit happened is not.
        expect(find.text('Edited'), findsOneWidget);
        expect(find.text(stampLabel(2)), findsNothing);
      });

      testWidgets('report a send that failed, without a time', (tester) async {
        messageListCubit.setState([
          stampFixture(1, sender: 1, status: UiMessageStatus.error),
          stampFixture(2, sender: 1),
          stampFixture(3, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject());

        // The failed send, the reader's own last message, and the end of the
        // chat.
        expect(find.byType(MessageMeta), findsNWidgets(3));
        expect(find.text('Failed to send'), findsOneWidget);
        expect(find.text(stampLabel(1)), findsNothing);
      });

      testWidgets('report a send nobody has received yet, without a time', (
        tester,
      ) async {
        messageListCubit.setState([
          stampFixture(1, sender: 1, status: UiMessageStatus.sent),
          stampFixture(2, sender: 1),
          stampFixture(3, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject());

        // The undelivered send, the reader's own last message, and the end of
        // the chat.
        expect(find.byType(MessageMeta), findsNWidgets(3));
        expect(find.text(stampLabel(1)), findsNothing);
      });

      testWidgets('leave an edit halfway up the history unreported', (
        tester,
      ) async {
        // Editing puts a message back to sent, however long ago the original
        // arrived. Message 1 is neither the end of the chat nor the reader's
        // own last, so the edit is all it has to report.
        messageListCubit.setState([
          stampFixture(1, sender: 1, edited: true, status: UiMessageStatus.sent),
          stampFixture(2, sender: 1),
          stampFixture(3, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject());

        expect(find.byType(MessageMeta), findsNWidgets(3));
        expect(find.text('Edited'), findsOneWidget);
        expect(find.text('Sent'), findsNothing);
      });

      testWidgets('report an edit that failed to go out', (tester) async {
        // A failed edit is the reader's business wherever it sits: the message
        // on screen is not the one the chat has.
        messageListCubit.setState([
          stampFixture(
            1,
            sender: 1,
            edited: true,
            status: UiMessageStatus.error,
          ),
          stampFixture(2, sender: 1),
          stampFixture(3, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject());

        expect(find.text('Edited'), findsOneWidget);
        expect(find.text('Failed to send'), findsOneWidget);
      });

      testWidgets('leave a send that landed with someone alone', (
        tester,
      ) async {
        messageListCubit.setState([
          stampFixture(1, sender: 1, status: UiMessageStatus.delivered),
          stampFixture(2, sender: 1),
          stampFixture(3, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject());

        // Only the reader's own last message and the end of the chat.
        expect(find.byType(MessageMeta), findsNWidgets(2));
        expect(find.text(stampLabel(1)), findsNothing);
      });

      testWidgets('are reachable on hover, beside the bubble and not on it', (
        tester,
      ) async {
        messageListCubit.setState([
          stampFixture(1, sender: 1),
          stampFixture(2, sender: 2),
          stampFixture(3, sender: 1),
          stampFixture(4, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject(platform: TargetPlatform.macOS));

        // Message 2 is not the end of the chat, so it carries no stamp of its
        // own and the pointer is the only way to its time.
        //
        // The clock alone, as under a bubble: the pointer reveals the same
        // stamp the meta row would have carried, never a day beside it.
        final label = stampLabel(2);
        final body = find.text('Message 2', findRichText: true);

        final pointer = await tester.createGesture(
          kind: PointerDeviceKind.mouse,
        );
        await pointer.addPointer(location: Offset.zero);
        addTearDown(pointer.removePointer);
        await pointer.moveTo(tester.getCenter(body));
        await tester.pump();

        // It waits out the hover delay, so skimming the conversation raises
        // nothing.
        expect(find.text(label), findsNothing);
        final bubbleBefore = tester.getRect(body);
        await tester.pump(const Duration(milliseconds: 700));
        expect(find.text(label), findsOneWidget);

        // An incoming row keeps its affordances to the right of the bubble, and
        // the time joins them there rather than covering the message.
        expect(
          tester.getRect(find.text(label)).left,
          greaterThan(bubbleBefore.right),
        );
        // The label rides in the row without taking any of it: the bubble sits
        // exactly where it did before the pointer arrived.
        expect(tester.getRect(body), bubbleBefore);
      });

      testWidgets('are reachable on hover on a row that takes no actions', (
        tester,
      ) async {
        // A failed send takes neither a reply nor a reaction, so the hover
        // buttons drop out of the row. The pointer is still the only way to
        // its time, so the label has to come up without them.
        messageListCubit.setState([
          stampFixture(1, sender: 1, status: UiMessageStatus.error),
          stampFixture(2, sender: 1),
          stampFixture(3, sender: 2),
        ]);

        await tester.pumpWidget(buildSubject(platform: TargetPlatform.macOS));

        final label = stampLabel(1);
        expect(find.text(label), findsNothing);

        final pointer = await tester.createGesture(
          kind: PointerDeviceKind.mouse,
        );
        await pointer.addPointer(location: Offset.zero);
        addTearDown(pointer.removePointer);
        await pointer.moveTo(
          tester.getCenter(find.text('Message 1', findRichText: true)),
        );
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 700));

        expect(find.text(label), findsOneWidget);
      });

      testWidgets('are withheld while newer messages remain unloaded', (
        tester,
      ) async {
        // Jumped into the middle of a history: neither the newest row on screen
        // nor the reader's own newest is the last of its kind in the chat.
        messageListCubit.setState([
          stampFixture(1, sender: 1),
          stampFixture(2, sender: 2),
        ], hasNewer: true);

        await tester.pumpWidget(buildSubject());

        expect(find.byType(MessageMeta), findsNothing);
      });
    });

    testWidgets('renders correctly when empty', (tester) async {
      messageListCubit.setState(const []);

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_empty.png'),
      );
    });

    testWidgets('renders correctly', (tester) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(messages);

      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_dark_mode.png'),
      );
    });

    testWidgets('renders correctly with attachments', (tester) async {
      tester.view.physicalSize = const Size(1080, 2400);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(attachmentMessages);
      when(
        () => attachmentsRepository.loadImageAttachment(
          attachmentId: any(named: 'attachmentId'),
          retryDownloadIfFailed: false,
          chunkEventCallback: any(named: "chunkEventCallback"),
        ),
      ).thenAnswer((_) async => Future.any([]));
      when(
        () => attachmentsRepository.statusStream(
          attachmentId: any(named: 'attachmentId'),
        ),
      ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_attachments.png'),
      );
    });

    testWidgets('renders correctly with blocked messages', (tester) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      final messageWithBobBlocked = [
        for (final message in messages)
          switch (message.message) {
            UiMessage_Content(field0: final content)
                when content.sender == 2.userId() =>
              message.copyWith(status: UiMessageStatus.hidden),
            _ => message,
          },
      ];
      messageListCubit.setState(messageWithBobBlocked);

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_blocked.png'),
      );
    });

    testWidgets('renders correctly with blocked messages in contact chat', (
      tester,
    ) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      final messageWithBobBlocked = [
        for (final message in messages) ...[
          if (message.sender == 1.userId()) message,
          if (message.sender == 2.userId())
            message.copyWith(status: UiMessageStatus.hidden),
        ],
      ];
      messageListCubit.setState(messageWithBobBlocked, isConnectionChat: true);

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_blocked_contact_chat.png'),
      );
    });

    testWidgets('renders jumbo emoji without bubble', (tester) async {
      tester.view.physicalSize = const Size(1080, 1350);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(jumboEmojiMessages);

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_jumbo_emoji.png'),
      );
    });

    testWidgets('renders a code block next to prose', (tester) async {
      tester.view.physicalSize = const Size(1080, 1500);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(codeBlockMessages);

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_code_block.png'),
      );
    });

    testWidgets('renders correctly with disabled read receipts', (
      tester,
    ) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(messages);
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(readReceipts: false));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_disabled_read_receipts.png'),
      );
    });

    testWidgets('renders unread divider', (tester) async {
      tester.view.physicalSize = const Size(1080, 2400);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      // Use a small subset so the golden stays compact. The divider lands at
      // index 2, mid-group for Eve, and has to break the group in two.
      messageListCubit.setState(
        messages.take(6).toList(),
        firstUnreadIndex: 2,
        unreadCount: 4,
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_unread_divider.png'),
      );
    });

    testWidgets('scrollToMessage loads and reaches an unloaded target', (
      tester,
    ) async {
      // Small viewport so most messages are off-screen.
      tester.view.physicalSize = const Size(400, 600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final manyMessages = List.generate(30, (i) {
        return UiChatMessage(
          id: (200 + i).messageId(),
          chatId: chatId,
          timestamp: DateTime(2023, 1, 1, 0, i),
          message: UiMessage_Content(
            UiContentMessage(
              sender: 2.userId(),
              sent: true,
              edited: false,
              content: UiMimiContent(
                plainBody: 'Message number $i',
                topicId: Uint8List(0),
                content: simpleMessage('Message number $i'),
                attachments: [],
              ),
            ),
          ),
          status: UiMessageStatus.sent,
          reactions: [],
        );
      });

      final targetMessage = UiChatMessage(
        id: 999.messageId(),
        chatId: chatId,
        timestamp: DateTime(2023, 1, 1, 2, 0),
        message: UiMessage_Content(
          UiContentMessage(
            sender: 2.userId(),
            sent: true,
            edited: false,
            content: UiMimiContent(
              plainBody: 'Loaded around target',
              topicId: Uint8List(0),
              content: simpleMessage('Loaded around target'),
              attachments: [],
            ),
          ),
        ),
        status: UiMessageStatus.sent,
        reactions: [],
      );

      MessageId? requestedMessageId;
      messageListCubit = MockMessageListCubit(
        initialMessages: manyMessages,
        onJumpToMessage: (messageId) async {
          requestedMessageId = messageId;
          messageListCubit.setState([...manyMessages, targetMessage]);
        },
      );

      await tester.pumpWidget(buildSubject());

      messageListCubit.emitCommand(
        MessageListCommand.scrollToId(messageId: targetMessage.id),
      );
      await tester.pump();

      // Pump enough frames for the iterative scroll to converge.
      for (var i = 0; i < 15; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }

      expect(requestedMessageId, targetMessage.id);
      expect(find.text('Loaded around target'), findsOneWidget);
    });

    testWidgets('marks the current visible message as read while scrolling', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final manyMessages = List.generate(30, (i) {
        return UiChatMessage(
          id: (400 + i).messageId(),
          chatId: chatId,
          timestamp: DateTime(2023, 1, 1, 0, i),
          message: UiMessage_Content(
            UiContentMessage(
              sender: 2.userId(),
              sent: true,
              edited: false,
              content: UiMimiContent(
                plainBody: 'Read marker message $i',
                topicId: Uint8List(0),
                content: simpleMessage('Read marker message $i'),
                attachments: [],
              ),
            ),
          ),
          status: UiMessageStatus.sent,
          reactions: [],
        );
      });

      messageListCubit.setState(manyMessages);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      reset(chatDetailsCubit);
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: 'untilMessageId'),
          untilTimestamp: any(named: 'untilTimestamp'),
        ),
      ).thenAnswer((_) => Future.value());

      tester
          .state<ScrollableState>(find.byType(Scrollable))
          .position
          .jumpTo(250);
      await tester.pump();

      verify(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: 'untilMessageId'),
          untilTimestamp: any(named: 'untilTimestamp'),
        ),
      ).called(1);
    });

    testWidgets(
      'lands on the first unread message and marks only what is visible with '
      'a large unread count',
      (tester) async {
        tester.view.physicalSize = const Size(400, 600);
        tester.view.devicePixelRatio = 1.0;
        addTearDown(() {
          tester.view.resetPhysicalSize();
          tester.view.resetDevicePixelRatio();
        });

        // Spaced far enough apart that each message keeps its own row height,
        // so the jump lands where the viewport arithmetic below expects it.
        UiChatMessage mk(int i) => UiChatMessage(
          id: (1000 + i).messageId(),
          chatId: chatId,
          timestamp: DateTime(2023, 1, 1, 0, i * 6),
          message: UiMessage_Content(
            UiContentMessage(
              sender: 2.userId(),
              sent: true,
              edited: false,
              content: UiMimiContent(
                plainBody: 'M$i',
                topicId: Uint8List(0),
                content: simpleMessage('M$i'),
                attachments: [],
              ),
            ),
          ),
          status: UiMessageStatus.sent,
          reactions: [],
        );

        final all = List.generate(80, mk);
        int indexOf(MessageId id) => all.indexWhere((m) => m.id == id);

        final initialWindow = all.sublist(0, 50);
        messageListCubit = MockMessageListCubit(
          onLoadNewer: () async =>
              messageListCubit.appendNewer(all.sublist(50), hasNewer: false),
        );

        final marked = <MessageId>[];
        when(
          () => chatDetailsCubit.markAsRead(
            untilMessageId: any(named: 'untilMessageId'),
            untilTimestamp: any(named: 'untilTimestamp'),
          ),
        ).thenAnswer((inv) {
          marked.add(inv.namedArguments[#untilMessageId] as MessageId);
          return Future.value();
        });

        await tester.pumpWidget(buildSubject());
        messageListCubit.setState(
          initialWindow,
          firstUnreadIndex: 10,
          hasNewer: true,
          revision: 1,
        );
        // Let the jump run and settle.
        for (var i = 0; i < 20; i++) {
          await tester.pump(const Duration(milliseconds: 50));
        }

        expect(find.text('M10'), findsOneWidget);

        expect(marked, isNotEmpty);
        for (final id in marked) {
          expect(
            indexOf(id),
            allOf(greaterThanOrEqualTo(10), lessThan(25)),
            reason:
                'marked $id (index ${indexOf(id)}) outside the visible '
                'first-unread region',
          );
        }
      },
    );

    testWidgets(
      'does not mark unseen newer messages as read when opening an unread chat',
      (tester) async {
        tester.view.physicalSize = const Size(400, 600);
        tester.view.devicePixelRatio = 1.0;
        addTearDown(() {
          tester.view.resetPhysicalSize();
          tester.view.resetDevicePixelRatio();
        });

        final manyMessages = List.generate(30, (i) {
          return UiChatMessage(
            id: (500 + i).messageId(),
            chatId: chatId,
            timestamp: DateTime(2023, 1, 1, 0, i),
            message: UiMessage_Content(
              UiContentMessage(
                sender: 2.userId(),
                sent: true,
                edited: false,
                content: UiMimiContent(
                  plainBody: 'Unread open message $i',
                  topicId: Uint8List(0),
                  content: simpleMessage('Unread open message $i'),
                  attachments: [],
                ),
              ),
            ),
            status: UiMessageStatus.sent,
            reactions: [],
          );
        });

        messageListCubit.setState(manyMessages, firstUnreadIndex: 15);

        await tester.pumpWidget(buildSubject());
        for (var i = 0; i < 15; i++) {
          await tester.pump(const Duration(milliseconds: 50));
        }

        verifyNever(
          () => chatDetailsCubit.markAsRead(
            untilMessageId: manyMessages.last.id,
            untilTimestamp: any(named: 'untilTimestamp'),
          ),
        );
        verify(
          () => chatDetailsCubit.markAsRead(
            untilMessageId: any(named: 'untilMessageId'),
            untilTimestamp: any(named: 'untilTimestamp'),
          ),
        ).called(greaterThanOrEqualTo(1));
      },
    );

    testWidgets('renders correctly with replies of various sizes', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(1080, 3000);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(replyMessages);

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_with_replies.png'),
      );
    });

    testWidgets('opens the who-reacted sheet when tapping a reaction chip', (
      tester,
    ) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      // The chip emoji is a painted glyph, not a Text, find it via semantics.
      // The handle must be disposed before the end of the test body: an
      // addTearDown callback runs too late for the framework's leak check.
      final semantics = tester.ensureSemantics();

      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject());

      // The chip merges the emoji and the reaction count ("🫪\n2") into one
      // semantics node, so match on the emoji rather than the whole label.
      await tester.tap(find.bySemanticsLabel(RegExp('🫪')));
      semantics.dispose();
      await tester.pump(kDoubleTapTimeout);
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_who_reacted_sheet.png'),
      );
    });

    testWidgets(
      'double-click selects a word on desktop',
      (tester) async {
        // A 1.0 ratio makes the whole list fit, so the target message is
        // always built and on screen regardless of the viewport anchor.
        tester.view.physicalSize = highTestSize;
        tester.view.devicePixelRatio = 1.0;
        addTearDown(() {
          tester.view.resetPhysicalSize();
          tester.view.resetDevicePixelRatio();
        });

        messageListCubit.setState(messages);

        await tester.pumpWidget(buildSubject());

        final target = find.textContaining('This is a delivered message');
        expect(
          find.ancestor(of: target, matching: find.byType(SelectableRegion)),
          findsOne,
        );
        final paragraph = tester.renderObject<RenderParagraph>(
          find.descendant(of: target, matching: find.byType(RichText)),
        );
        // The tile can be built but laid out below the fold, where taps
        // land on nothing.
        await tester.ensureVisible(target);
        await tester.pumpAndSettle();
        final center = tester.getCenter(target);

        final gesture = await tester.startGesture(
          center,
          kind: PointerDeviceKind.mouse,
        );
        addTearDown(gesture.removePointer);
        await tester.pump();
        await gesture.up();
        await tester.pump(const Duration(milliseconds: 50));
        await gesture.down(center);
        await tester.pump();
        await gesture.up();
        await tester.pump(kDoubleTapTimeout);
        await tester.pumpAndSettle();

        // A double-tap recognizer on the bubble would win the gesture arena
        // and swallow the second click, leaving no word selection.
        expect(paragraph.selections, hasLength(1));
        expect(paragraph.selections.single.isCollapsed, isFalse);
        expect(find.byType(ReactionBar), findsNothing);
      },
      variant: TargetPlatformVariant.only(TargetPlatform.macOS),
    );
  });
}
