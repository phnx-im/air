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

final sashaId = ownId;
final sigridId = 2.userId();
final deeptiId = 3.userId();
final juleId = 4.userId();
final fredericId = 5.userId();
final amaraId = 6.userId();
final jacobId = 7.userId();
final luisId = 8.userId();
final kristijanId = 9.userId();
final josefinaId = 10.userId();
final lucaId = 11.userId();
final lukasId = 12.userId();

final sigridChatId = 1.chatId();
final roomiesChatId = 2.chatId();
final shiftSwapsChatId = 3.chatId();
final amaraChatId = 4.chatId();
final lukasChatId = 5.chatId();
final communityMealsChatId = 6.chatId();
final jacobChatId = 7.chatId();
final neighborhoodCleaningChatId = 8.chatId();
final luisChatId = 9.chatId();
final kristijanChatId = 10.chatId();

final sashaProfilePicture = _loadImageSync('test/assets/images/sasha.jpg');
final sigridProfilePicture = _loadImageSync('test/assets/images/sigridh.jpg');
final deeptiProfilePicture = _loadImageSync('test/assets/images/deepti.jpg');
final juleProfilePicture = _loadImageSync('test/assets/images/jule.jpg');
final fredericProfilePicture = _loadImageSync(
  'test/assets/images/frederic.jpg',
);
final amaraProfilePicture = _loadImageSync('test/assets/images/amara.jpg');
final jacobProfilePicture = _loadImageSync('test/assets/images/jacob.jpg');
final luisProfilePicture = _loadImageSync('test/assets/images/luis.jpg');
final kristijanProfilePicture = _loadImageSync(
  'test/assets/images/kristijan.jpg',
);
final josefinaProfilePicture = _loadImageSync(
  'test/assets/images/josefina.jpg',
);
final lucaProfilePicture = _loadImageSync('test/assets/images/luca.jpg');
final lukasProfilePicture = _loadImageSync('test/assets/images/lukas.jpg');

final roomiesProfilePicture = _loadImageSync(
  'test/assets/images/group-roomies.jpg',
);
final shiftSwapsProfilePicture = _loadImageSync(
  'test/assets/images/group-dinner-club.jpg',
);
final communityMealsProfilePicture = _loadImageSync(
  'test/assets/images/group-market-crew.jpg',
);
final neighborhoodCleaningProfilePicture = _loadImageSync(
  'test/assets/images/neighborhood-cleanup.jpg',
);

final cookiesAttachmentImage = _loadImageSync('test/assets/images/cookies.jpg');
final hikeLakeViewImage = _loadImageSync(
  'test/assets/images/hike-lake-view.jpg',
);
final hikeDogImage = _loadImageSync('test/assets/images/hike-dog.jpg');
final hikePineImage = _loadImageSync('test/assets/images/hike-pine.jpg');

final hikeLakeViewAttachmentId = 101.attachmentId();
final hikeDogAttachmentId = 102.attachmentId();
final hikePineAttachmentId = 103.attachmentId();

final sashaProfile = UiUserProfile(
  userId: sashaId,
  displayName: 'Sasha',
  profilePicture: sashaProfilePicture,
);
final sigridProfile = UiUserProfile(
  userId: sigridId,
  displayName: 'Sigrid H',
  profilePicture: sigridProfilePicture,
);
final deeptiProfile = UiUserProfile(
  userId: deeptiId,
  displayName: 'Deepti',
  profilePicture: deeptiProfilePicture,
);
final juleProfile = UiUserProfile(
  userId: juleId,
  displayName: 'Jule',
  profilePicture: juleProfilePicture,
);
final fredericProfile = UiUserProfile(
  userId: fredericId,
  displayName: 'Frederic',
  profilePicture: fredericProfilePicture,
);
final amaraProfile = UiUserProfile(
  userId: amaraId,
  displayName: 'Amara',
  profilePicture: amaraProfilePicture,
);
final jacobProfile = UiUserProfile(
  userId: jacobId,
  displayName: 'Jacob Brooks',
  profilePicture: jacobProfilePicture,
);
final luisProfile = UiUserProfile(
  userId: luisId,
  displayName: 'Luis',
  profilePicture: luisProfilePicture,
);
final kristijanProfile = UiUserProfile(
  userId: kristijanId,
  displayName: 'Kristijan P',
  profilePicture: kristijanProfilePicture,
);
final josefinaProfile = UiUserProfile(
  userId: josefinaId,
  displayName: 'Josefina',
  profilePicture: josefinaProfilePicture,
);
final lucaProfile = UiUserProfile(
  userId: lucaId,
  displayName: 'Luca',
  profilePicture: lucaProfilePicture,
);
final lukasProfile = UiUserProfile(
  userId: lukasId,
  displayName: 'Lukas',
  profilePicture: lukasProfilePicture,
);

final userProfiles = [
  sashaProfile,
  sigridProfile,
  deeptiProfile,
  juleProfile,
  fredericProfile,
  amaraProfile,
  jacobProfile,
  luisProfile,
  kristijanProfile,
  josefinaProfile,
  lucaProfile,
  lukasProfile,
];

var messageIdx = 1;

final now = DateTime.now();

final chats = [
  // Sigrid H
  UiChatDetails(
    id: sigridChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(sigridProfile),
    unreadMessages: 2,
    lastUsed: now,
    lastMessage: _lastChatMessage(sigridChatId, sigridId, 'Can you make it?'),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Roomies
  UiChatDetails(
    id: roomiesChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Group(
      UiChatAttributes(title: 'Roomies', picture: roomiesProfilePicture),
    ),
    unreadMessages: 1,
    lastUsed: now.subtract(const Duration(minutes: 5)),
    lastMessage: _lastChatMessage(
      roomiesChatId,
      deeptiId,
      '😍😍 You really love those cookies.',
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Shift swaps
  UiChatDetails(
    id: shiftSwapsChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Group(
      UiChatAttributes(title: 'Shift swaps', picture: shiftSwapsProfilePicture),
    ),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(minutes: 15)),
    lastMessage: _lastChatMessage(
      shiftSwapsChatId,
      sashaId,
      'I could trade my Sunday opening for your Friday close!',
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Amara
  UiChatDetails(
    id: amaraChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(amaraProfile),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(minutes: 20)),
    lastMessage: _lastChatMessage(
      amaraChatId,
      amaraId,
      "I'm going to cast on tonight!",
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Lukas
  UiChatDetails(
    id: lukasChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(lukasProfile),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(minutes: 30)),
    lastMessage: _lastChatMessage(
      lukasChatId,
      lukasId,
      "I'll be in your area later, so I'll drop off a couple pieces at your apartment!",
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Community meals crew
  UiChatDetails(
    id: communityMealsChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Group(
      UiChatAttributes(
        title: 'Community meals crew',
        picture: communityMealsProfilePicture,
      ),
    ),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(days: 1)),
    lastMessage: _lastChatMessage(
      communityMealsChatId,
      fredericId,
      "Here's the plan recap: Amara and Deepti do the shopping, I'll bring the kitchen gear as usual, and we all meet at the park at 10am to prep so we're ready to serve by noon.",
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Jacob Brooks
  UiChatDetails(
    id: jacobChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(jacobProfile),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(days: 1, minutes: 10)),
    lastMessage: _lastChatMessage(jacobChatId, sashaId, 'Welcome to Air!'),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Neighborhood cleaning
  UiChatDetails(
    id: neighborhoodCleaningChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Group(
      UiChatAttributes(
        title: 'Neighborhood cleaning',
        picture: neighborhoodCleaningProfilePicture,
      ),
    ),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(days: 1, minutes: 20)),
    lastMessage: _lastChatMessage(
      neighborhoodCleaningChatId,
      kristijanId,
      'Wonderful! We actually have a little more budget for printing than last month.',
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Luis
  UiChatDetails(
    id: luisChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(luisProfile),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(days: 1, minutes: 30)),
    lastMessage: _lastChatMessage(
      luisChatId,
      luisId,
      "Eagle Ridge. You have to come next time, it's so beautiful.",
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Kristijan P
  UiChatDetails(
    id: kristijanChatId,
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(kristijanProfile),
    unreadMessages: 0,
    lastUsed: now.subtract(const Duration(days: 7)),
    lastMessage: _lastChatMessage(
      kristijanChatId,
      kristijanId,
      "That's so great! I started this monthly event a few years ago just for that reason.",
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
];

final chatIds = chats.map((chat) => chat.id).toList();

UiChatMessage _lastChatMessage(ChatId chatId, UiUserId senderId, String body) =>
    UiChatMessage(
      id: (messageIdx++).messageId(),
      chatId: chatId,
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
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
      status: UiMessageStatus.sent,
      reactions: [],
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

UiChatMessage _textMessage(
  ChatId chatId,
  UiUserId senderId,
  String body,
  DateTime timestamp, {
  UiMessageStatus status = UiMessageStatus.sent,
  List<UiReaction> reactions = const [],
}) => UiChatMessage(
  id: (messageIdx++).messageId(),
  chatId: chatId,
  timestamp: timestamp,
  message: UiMessage_Content(
    UiContentMessage(
      sender: senderId,
      sent: true,
      edited: false,
      content: UiMimiContent(
        plainBody: "",
        topicId: Uint8List(0),
        content: _simpleMessage(body),
        attachments: [],
      ),
    ),
  ),
  status: status,
  reactions: reactions,
);

UiChatMessage _imageMessage(
  ChatId chatId,
  UiUserId senderId,
  String body,
  String filename,
  ImageData image,
  DateTime timestamp, {
  AttachmentId? attachmentId,
  UiMessageStatus status = UiMessageStatus.sent,
  List<UiReaction> reactions = const [],
}) => UiChatMessage(
  id: (messageIdx++).messageId(),
  chatId: chatId,
  timestamp: timestamp,
  message: UiMessage_Content(
    UiContentMessage(
      sender: senderId,
      sent: true,
      edited: false,
      content: UiMimiContent(
        plainBody: "",
        topicId: Uint8List(0),
        content: _simpleMessage(body),
        attachments: [
          UiAttachment(
            attachmentId: attachmentId ?? (messageIdx).attachmentId(),
            filename: filename,
            contentType: "image/jpeg",
            size: image.data.length,
            description: filename,
            imageMetadata: const UiImageMetadata(
              blurhash: "LGDv.p%L00kC~qjF4nWCIARjIVj[",
              width: 1080,
              height: 1080,
            ),
          ),
        ],
        firstAttachmentType: UiAttachmentType.image,
      ),
    ),
  ),
  status: status,
  reactions: reactions,
);

final luisMessages = [
  _textMessage(
    luisChatId,
    sashaId,
    'Wait, so you actually liked the new album?',
    now.subtract(const Duration(hours: 76)),
    status: UiMessageStatus.read,
  ),
  _textMessage(
    luisChatId,
    luisId,
    'Track 4 is unreal.',
    now.subtract(const Duration(hours: 75, minutes: 52)),
  ),
  _textMessage(
    luisChatId,
    luisId,
    "I think the best thing they've done.",
    now.subtract(const Duration(hours: 75, minutes: 51)),
  ),
  _textMessage(
    luisChatId,
    sashaId,
    'I told you the synths would win you over eventually.',
    now.subtract(const Duration(hours: 75, minutes: 50)),
    status: UiMessageStatus.read,
  ),
  _textMessage(
    luisChatId,
    luisId,
    'Haha, yes, you were right!',
    now.subtract(const Duration(hours: 75, minutes: 49)),
  ),
  _textMessage(
    luisChatId,
    luisId,
    'Heads up, they just announced a show here in November. Want to go?',
    now.subtract(const Duration(hours: 47)),
  ),
  _textMessage(
    luisChatId,
    sashaId,
    'Obviously. Setting a reminder for the presale right now.',
    now.subtract(const Duration(hours: 46, minutes: 58)),
    status: UiMessageStatus.read,
    reactions: [
      UiReaction(emoji: '🤘', users: [luisId]),
    ],
  ),
  _textMessage(
    luisChatId,
    luisId,
    'Also, I took the album on my hike this morning. The perfect soundtrack 🎧',
    now.subtract(const Duration(hours: 46, minutes: 56)),
  ),
  _imageMessage(
    luisChatId,
    luisId,
    '',
    'hike-lake-view.jpg',
    hikeLakeViewImage,
    now.subtract(const Duration(hours: 46, minutes: 55)),
    attachmentId: hikeLakeViewAttachmentId,
  ),
  _imageMessage(
    luisChatId,
    luisId,
    '',
    'hike-dog.jpg',
    hikeDogImage,
    now.subtract(const Duration(hours: 46, minutes: 54)),
    attachmentId: hikeDogAttachmentId,
  ),
  _imageMessage(
    luisChatId,
    luisId,
    '',
    'hike-pine.jpg',
    hikePineImage,
    now.subtract(const Duration(hours: 46, minutes: 53)),
    attachmentId: hikePineAttachmentId,
    reactions: [
      UiReaction(emoji: '🤩', users: [sashaId]),
    ],
  ),
  _textMessage(
    luisChatId,
    sashaId,
    'Okay, that view!! Where is this?',
    now.subtract(const Duration(hours: 46, minutes: 49)),
    status: UiMessageStatus.read,
  ),
  _textMessage(
    luisChatId,
    luisId,
    "Eagle Ridge. You have to come next time, it's so beautiful.",
    now.subtract(const Duration(hours: 46, minutes: 46)),
  ),
];

final roomiesMembers = [sashaId, deeptiId, juleId];

final roomiesMessages = [
  _textMessage(
    roomiesChatId,
    deeptiId,
    'Did anyone take the trash out last night?',
    now.subtract(const Duration(minutes: 29)),
  ),
  _textMessage(
    roomiesChatId,
    juleId,
    'I thought it was your week 😬',
    now.subtract(const Duration(minutes: 27)),
  ),
  _textMessage(
    roomiesChatId,
    deeptiId,
    'Absolutely not.',
    now.subtract(const Duration(minutes: 27)),
  ),
  _textMessage(
    roomiesChatId,
    sashaId,
    "It was Jule's week. I did it last week AND the week before.",
    now.subtract(const Duration(minutes: 24)),
    status: UiMessageStatus.read,
  ),
  _textMessage(
    roomiesChatId,
    juleId,
    "I'll do it tonight. Sorry!",
    now.subtract(const Duration(minutes: 23)),
  ),
  _textMessage(
    roomiesChatId,
    deeptiId,
    '🫶',
    now.subtract(const Duration(minutes: 23)),
  ),
  _textMessage(
    roomiesChatId,
    sashaId,
    'Also, are we doing dinner together on Friday? I was thinking I could make that lentil salad with the pickled cherries again.',
    now.subtract(const Duration(minutes: 21)),
    status: UiMessageStatus.read,
    reactions: [
      UiReaction(emoji: '❤️', users: [deeptiId, juleId]),
    ],
  ),
  _textMessage(
    roomiesChatId,
    juleId,
    'Yes, please!',
    now.subtract(const Duration(minutes: 20)),
  ),
  _textMessage(
    roomiesChatId,
    deeptiId,
    '🙌',
    now.subtract(const Duration(minutes: 20)),
  ),
  _textMessage(
    roomiesChatId,
    deeptiId,
    'Can we invite Jana? She’s been having a rough week.',
    now.subtract(const Duration(minutes: 19)),
  ),
  _textMessage(
    roomiesChatId,
    sashaId,
    'Obviously! The more the merrier.',
    now.subtract(const Duration(minutes: 18)),
    status: UiMessageStatus.read,
  ),
  _textMessage(
    roomiesChatId,
    sashaId,
    'Tell her to bring cookies from the bakery near her place, and she’s in 🎪',
    now.subtract(const Duration(minutes: 18)),
    status: UiMessageStatus.read,
  ),
  _textMessage(
    roomiesChatId,
    juleId,
    'CHOCOLATE CHIP, PLEASE!!! 🍪🍪🍪 I’m already drooling.',
    now.subtract(const Duration(minutes: 16)),
  ),
  _imageMessage(
    roomiesChatId,
    juleId,
    '',
    'cookies.jpg',
    cookiesAttachmentImage,
    now.subtract(const Duration(minutes: 15)),
    reactions: [
      UiReaction(emoji: '🤤', users: [deeptiId]),
    ],
  ),
  _textMessage(
    roomiesChatId,
    deeptiId,
    '😍😍 You really love those cookies.',
    now.subtract(const Duration(minutes: 14)),
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
