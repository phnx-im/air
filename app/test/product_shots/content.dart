// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'dart:typed_data';

import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:crypto/crypto.dart';

import '../helpers.dart';

const ownIdx = 1;
final ownId = ownIdx.userId();

final samId = ownId;
final fredId = 2.userId();
final jessicaId = 3.userId();
final daveId = 4.userId();
final frankId = 5.userId();
final alexId = 6.userId();
final ireneId = 7.userId();
final kamalId = 8.userId();

final samChatId = 1.chatId();
final fredChatId = 2.chatId();
final jessicaChatId = 3.chatId();
final daveChatId = 4.chatId();
final frankChatId = 5.chatId();
final alexChatId = 6.chatId();
final ireneChatId = 7.chatId();
final kamalChatId = 8.chatId();

final scienceClubId = 10.chatId();
final gardeningPartyId = 11.chatId();
final dinnerPartyId = 12.chatId();

final samProfilePicture = _loadImageSync('test/assets/images/sam.jpg');
final fredProfilePicture = _loadImageSync('test/assets/images/fred.jpg');
final jessicaProfilePicture = _loadImageSync('test/assets/images/jessica.jpg');
final daveProfilePicture = _loadImageSync('test/assets/images/dave.jpg');
final frankProfilePicture = _loadImageSync('test/assets/images/frank.jpg');
final alexProfilePicture = _loadImageSync('test/assets/images/alex.jpg');
final ireneProfilePicture = _loadImageSync('test/assets/images/irene.jpg');
final kamalProfilePicture = _loadImageSync('test/assets/images/kamal.jpg');

final scienceClubProfilePicture = _loadImageSync(
  'test/assets/images/science-club.jpg',
);
final gardeningPartyProfilePicture = _loadImageSync(
  'test/assets/images/gardening-group.jpg',
);
final dinnerPartyProfilePicture = _loadImageSync(
  'test/assets/images/dinner-party.jpg',
);

final jupiterAttachmentImage = _loadImageSync('test/assets/images/jupiter.jpg');

final samProfile = UiUserProfile(
  userId: samId,
  displayName: 'Sam',
  profilePicture: samProfilePicture,
);
final fredProfile = UiUserProfile(
  userId: fredId,
  displayName: 'Fred',
  profilePicture: fredProfilePicture,
);
final jessicaProfile = UiUserProfile(
  userId: jessicaId,
  displayName: 'Jessica',
  profilePicture: jessicaProfilePicture,
);
final daveProfile = UiUserProfile(
  userId: daveId,
  displayName: 'Dave',
  profilePicture: daveProfilePicture,
);
final frankProfile = UiUserProfile(
  userId: frankId,
  displayName: 'Frank',
  profilePicture: frankProfilePicture,
);
final alexProfile = UiUserProfile(
  userId: alexId,
  displayName: 'Alex',
  profilePicture: alexProfilePicture,
);
final ireneProfile = UiUserProfile(
  userId: ireneId,
  displayName: 'Irene',
  profilePicture: ireneProfilePicture,
);
final kamalProfile = UiUserProfile(
  userId: kamalId,
  displayName: 'Kamal',
  profilePicture: kamalProfilePicture,
);

final userProfiles = [
  samProfile,
  fredProfile,
  jessicaProfile,
  daveProfile,
  frankProfile,
  alexProfile,
  ireneProfile,
  kamalProfile,
];

var messageIdx = 1;

final now = DateTime.now();

final chats = [
  // Fred
  UiChatDetails(
    id: fredChatId,
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(fredProfile),
    unreadMessages: 1,
    messagesCount: 1,
    attributes: const UiChatAttributes(title: 'Fred', picture: null),
    lastUsed: now.toIso8601String(),
    lastMessage: _lastChatMessage(
      fredChatId,
      fredId,
      'My favorite planet is Jupiter. But I also like Venus a lot.',
    ),
  ),
  // Jessica
  UiChatDetails(
    id: jessicaChatId,
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(jessicaProfile),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: const UiChatAttributes(title: 'Jessica', picture: null),
    lastUsed: now.subtract(const Duration(minutes: 1)).toIso8601String(),
    lastMessage: _lastChatMessage(
      jessicaChatId,
      jessicaId,
      "What is the recipe for the cake you made?",
    ),
  ),
  // Science club
  UiChatDetails(
    id: scienceClubId,
    status: const UiChatStatus.active(),
    chatType: const UiChatType_Group(),
    unreadMessages: 0,
    messagesCount: 0,
    attributes: UiChatAttributes(
      title: 'Science club',
      picture: scienceClubProfilePicture,
    ),
    lastUsed: now.subtract(const Duration(minutes: 10)).toIso8601String(),
    lastMessage: _lastChatMessage(
      scienceClubId,
      samId,
      "Riemanian Zeta function is one of the most important mathematical functions in the history of mathematics.",
    ),
  ),
  // Dave
  UiChatDetails(
    id: daveChatId,
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(daveProfile),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: const UiChatAttributes(title: 'Dave', picture: null),
    lastUsed: now.subtract(const Duration(minutes: 15)).toIso8601String(),
    lastMessage: _lastChatMessage(
      daveChatId,
      daveId,
      'I have to tell you all about my weekend...',
    ),
  ),
  // Gardening club
  UiChatDetails(
    id: gardeningPartyId,
    status: const UiChatStatus.active(),
    chatType: const UiChatType_Group(),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: UiChatAttributes(
      title: 'Gardening club',
      picture: gardeningPartyProfilePicture,
    ),
    lastUsed: now.subtract(const Duration(minutes: 20)).toIso8601String(),
    lastMessage: _lastChatMessage(
      gardeningPartyId,
      samId,
      "Last year I grew 5 different varieties of carrots! Let me if I can find a good primer about how to grow them...",
    ),
  ),
  // Frank
  UiChatDetails(
    id: frankChatId,
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(frankProfile),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: const UiChatAttributes(title: 'Frank', picture: null),
    lastUsed: now.subtract(const Duration(minutes: 30)).toIso8601String(),
    lastMessage: _lastChatMessage(
      frankChatId,
      frankId,
      'Going to the store. Need anything?',
    ),
  ),
  // Alex
  UiChatDetails(
    id: alexChatId,
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(alexProfile),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: const UiChatAttributes(title: 'Alex', picture: null),
    lastUsed: now.subtract(const Duration(days: 1)).toIso8601String(),
    lastMessage: _lastChatMessage(alexChatId, alexId, "See you there."),
  ),
  // Irene
  UiChatDetails(
    id: ireneChatId,
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(ireneProfile),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: const UiChatAttributes(title: 'Irene', picture: null),
    lastUsed: now.subtract(const Duration(days: 1)).toIso8601String(),
    lastMessage: _lastChatMessage(
      ireneChatId,
      ireneId,
      "The nearest star is Proxima Centauri.",
    ),
  ),
  // Dinner party
  UiChatDetails(
    id: dinnerPartyId,
    status: const UiChatStatus.active(),
    chatType: const UiChatType_Group(),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: UiChatAttributes(
      title: 'Dinner party',
      picture: dinnerPartyProfilePicture,
    ),
    lastUsed: now.subtract(const Duration(days: 1)).toIso8601String(),
    lastMessage: _lastChatMessage(
      dinnerPartyId,
      ownId,
      "Sorry, I can't join the party. I'm going to the movies already.",
    ),
  ),
  // Kamal
  UiChatDetails(
    id: kamalChatId,
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(kamalProfile),
    unreadMessages: 0,
    messagesCount: 1,
    attributes: const UiChatAttributes(title: 'Kamal', picture: null),
    lastUsed: now.subtract(const Duration(days: 1)).toIso8601String(),
    lastMessage: _lastChatMessage(
      kamalChatId,
      ownId,
      "Hey Kamal, I'm going to the movies with my friends. Want to come with us?",
    ),
  ),
];

final chatIds = chats.map((chat) => chat.id).toList();

UiChatMessage _lastChatMessage(ChatId chatId, UiUserId senderId, String body) =>
    UiChatMessage(
      id: (messageIdx++).messageId(),
      chatId: chatId,
      timestamp: '2023-01-01T00:00:00.000Z',
      message: UiMessage_Content(
        UiContentMessage(
          sender: senderId,
          sent: true,
          edited: false,
          content: UiMimiContent(
            plainBody: body,
            topicId: Uint8List(0),
            content: _simpleMessage(body),
            attachments: [],
          ),
        ),
      ),
      position: UiFlightPosition.single,
      status: UiMessageStatus.sent,
    );

MessageContent _simpleMessage(String msg) {
  return MessageContent(
    elements: [
      RangedBlockElement(
        start: 0,
        end: msg.length,
        element: BlockElement_Paragraph([
          RangedInlineElement(
            start: 0,
            end: msg.length,
            element: InlineElement_Text(msg),
          ),
        ]),
      ),
    ],
  );
}

final fredMessages = [
  UiChatMessage(
    id: (messageIdx++).messageId(),
    chatId: fredChatId,
    timestamp: now.subtract(const Duration(minutes: 1)).toIso8601String(),
    message: UiMessage_Content(
      UiContentMessage(
        sender: ownId,
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "",
          topicId: Uint8List(0),
          content: _simpleMessage("Hey Fred, what’s your favorite planet?"),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.read,
  ),
  UiChatMessage(
    id: (messageIdx++).messageId(),
    chatId: fredChatId,
    timestamp: now.toIso8601String(),
    message: UiMessage_Content(
      UiContentMessage(
        sender: fredId,
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "",
          topicId: Uint8List(0),
          content: _simpleMessage(
            "My favorite planet is Jupiter. But I also like Venus a lot.",
          ),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.start,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: (messageIdx++).messageId(),
    chatId: fredChatId,
    timestamp: now.toIso8601String(),
    message: UiMessage_Content(
      UiContentMessage(
        sender: fredId,
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "",
          topicId: Uint8List(0),
          content: _simpleMessage("Isn't it beautiful?"),
          attachments: [
            UiAttachment(
              attachmentId: 1.attachmentId(),
              filename: "jupiter.jpg",
              contentType: "image/jpeg",
              size: jupiterAttachmentImage.data.length,
              description: "Jupiter",
              imageMetadata: const UiImageMetadata(
                blurhash: "LGDv.p%L00kC~qjF4nWCIARjIVj[",
                width: 1080,
                height: 800,
              ),
            ),
          ],
        ),
      ),
    ),
    position: UiFlightPosition.end,
    status: UiMessageStatus.sent,
  ),
];

final gardeningPartyMembers = [samId, fredId, jessicaId];

final gardeningPartyMessages = [
  UiChatMessage(
    id: (messageIdx++).messageId(),
    chatId: gardeningPartyId,
    timestamp: now.subtract(const Duration(minutes: 23)).toIso8601String(),
    message: UiMessage_Content(
      UiContentMessage(
        sender: samId,
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "",
          topicId: Uint8List(0),
          content: _simpleMessage(
            'Does anyone know the best time of year to plant carrots?',
          ),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.read,
  ),
  UiChatMessage(
    id: (messageIdx++).messageId(),
    chatId: gardeningPartyId,
    timestamp: now.subtract(const Duration(minutes: 21)).toIso8601String(),
    message: UiMessage_Content(
      UiContentMessage(
        sender: fredId,
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "",
          topicId: Uint8List(0),
          content: _simpleMessage("I don't know, I've never tried it."),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: (messageIdx++).messageId(),
    chatId: gardeningPartyId,
    timestamp: now.subtract(const Duration(minutes: 20)).toIso8601String(),
    message: UiMessage_Content(
      UiContentMessage(
        sender: jessicaId,
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "",
          topicId: Uint8List(0),
          content: _simpleMessage(
            'Last year I grew 5 different varieties of carrots! Let me if I can find a good primer about how to grow them...',
          ),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
];

ImageData _loadImageSync(String path) {
  final bytes = _getProjectFile(path).readAsBytesSync();
  final hash = sha256.convert(bytes).toString();
  return ImageData(data: bytes, hash: hash);
}

File _getProjectFile(String path) {
  var dir = Directory.current;
  while (!dir.listSync().any(
    (entity) => entity.path.endsWith('pubspec.yaml'),
  )) {
    dir = dir.parent;
  }
  return File('${dir.path}/$path');
}
