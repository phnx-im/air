// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'dart:typed_data';

import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:crypto/crypto.dart';
import 'package:yaml/yaml.dart';

import '../helpers.dart';

const ownIdx = 1;
final ownId = ownIdx.userId();

final _content = _Content.load();

final userProfiles = _content.userProfiles;
final chats = _content.chats;

final luisId = _content.userId('luis');
final luisMessages = _content.transcript('luis');

final roomiesMembers = _content.members('roomies');
final roomiesMessages = _content.transcript('roomies');

final cookiesAttachmentImage = _content.image('cookies.jpg');
final hikeLakeViewImage = _content.image('hike-lake-view.jpg');
final hikeDogImage = _content.image('hike-dog.jpg');
final hikePineImage = _content.image('hike-pine.jpg');

final hikeLakeViewAttachmentId = _content.attachmentId('hike-lake-view.jpg');
final hikeDogAttachmentId = _content.attachmentId('hike-dog.jpg');
final hikePineAttachmentId = _content.attachmentId('hike-pine.jpg');

/// Loads the persona, contacts, chats, and message transcripts described in
/// `content.yaml` and turns them into the FRB types the product shot tests
/// pump into the app's widgets.
class _Content {
  _Content._(this._yaml);

  factory _Content.load() {
    final text = _getProjectFile(
      'test/product_shots/content.yaml',
    ).readAsStringSync();
    final content = _Content._(loadYaml(text) as YamlMap);
    // Order matters: chats and transcripts reference user profiles by key.
    content.userProfiles = content._loadUserProfiles();
    content.chats = content._loadChats();
    content._transcripts = content._loadTranscripts();
    return content;
  }

  final YamlMap _yaml;

  final Map<String, UiUserId> _userIds = {};
  final Map<String, UiUserProfile> _profiles = {};
  final Map<String, ChatId> _chatIds = {};
  final Map<String, List<UiUserId>> _members = {};
  final Map<String, ImageData> _images = {};
  final Map<String, AttachmentId> _attachmentIds = {};

  int _nextUserId = 2;
  int _nextChatId = 1;
  int _nextAttachmentId = 1;
  int _messageIdx = 1;

  late final List<UiUserProfile> userProfiles;
  late final List<UiChatDetails> chats;
  late final Map<String, List<UiChatMessage>> _transcripts;

  UiUserId userId(String key) =>
      _userIds.putIfAbsent(key, () => (_nextUserId++).userId());

  ChatId _chatId(String key) =>
      _chatIds.putIfAbsent(key, () => (_nextChatId++).chatId());

  ImageData image(String filename) => _images.putIfAbsent(
    filename,
    () => _loadImageSync('test/assets/images/$filename'),
  );

  AttachmentId attachmentId(String filename) => _attachmentIds.putIfAbsent(
    filename,
    () => (_nextAttachmentId++).attachmentId(),
  );

  List<UiUserId> members(String chatKey) => _members[chatKey]!;

  List<UiChatMessage> transcript(String chatKey) => _transcripts[chatKey]!;

  List<UiUserProfile> _loadUserProfiles() {
    final own = _yaml['own'] as YamlMap;
    final ownKey = own['key'] as String;
    _userIds[ownKey] = ownId;
    final profiles = [_profile(own, id: ownId)];
    _profiles[ownKey] = profiles.single;

    for (final contact in _yaml['contacts'] as YamlList) {
      final map = contact as YamlMap;
      final key = map['key'] as String;
      final profile = _profile(map, id: userId(key));
      _profiles[key] = profile;
      profiles.add(profile);
    }
    return profiles;
  }

  UiUserProfile _profile(YamlMap map, {required UiUserId id}) => UiUserProfile(
    userId: id,
    displayName: map['name'] as String,
    profilePicture: image(map['avatar'] as String),
  );

  List<UiChatDetails> _loadChats() {
    final now = DateTime.now();
    final result = <UiChatDetails>[];
    for (final entry in _yaml['chats'] as YamlList) {
      final map = entry as YamlMap;
      final key = map['key'] as String;
      final chatId = _chatId(key);

      if (map['members'] != null) {
        _members[key] = [
          for (final memberKey in map['members'] as YamlList)
            userId(memberKey as String),
        ];
      }

      final lastMessage = map['lastMessage'] as YamlMap;
      result.add(
        UiChatDetails(
          id: chatId,
          status: const UiChatStatus.active(),
          isApq: false,
          chatType: _chatType(map),
          unreadMessages: map['unread'] as int,
          lastUsed: now.subtract(_duration(map['lastUsed'] as YamlMap)),
          lastMessage: _lastChatMessage(
            chatId,
            userId(lastMessage['sender'] as String),
            lastMessage['text'] as String,
          ),
          mutedUntil: null,
          pendingCommitFailed: false,
        ),
      );
    }
    return result;
  }

  UiChatType _chatType(YamlMap map) {
    switch (map['type'] as String) {
      case 'direct':
        return UiChatType_Connection(_profiles[map['contact']]!);
      case 'group':
        return UiChatType_Group(
          UiChatAttributes(
            title: map['title'] as String,
            picture: image(map['avatar'] as String),
          ),
        );
      default:
        throw ArgumentError('Unknown chat type: ${map['type']}');
    }
  }

  Map<String, List<UiChatMessage>> _loadTranscripts() {
    final now = DateTime.now();
    final transcripts = _yaml['transcripts'] as YamlMap;
    final result = <String, List<UiChatMessage>>{};
    for (final rawKey in transcripts.keys) {
      final chatKey = rawKey as String;
      final chatId = _chatId(chatKey);
      result[chatKey] = [
        for (final entry in transcripts[chatKey] as YamlList)
          _message(chatId, entry as YamlMap, now),
      ];
    }
    return result;
  }

  UiChatMessage _message(ChatId chatId, YamlMap map, DateTime now) {
    final sender = userId(map['sender'] as String);
    final timestamp = now.subtract(_duration(map['time'] as YamlMap));
    final status = map['status'] == 'read'
        ? UiMessageStatus.read
        : UiMessageStatus.sent;
    final reactions = [
      for (final reaction in (map['reactions'] as YamlList?) ?? const [])
        _reaction(reaction as YamlMap),
    ];

    final filename = map['image'] as String?;
    if (filename != null) {
      return _imageMessage(
        chatId,
        sender,
        filename,
        image(filename),
        timestamp,
        attachmentId: attachmentId(filename),
        status: status,
        reactions: reactions,
      );
    }
    return _textMessage(
      chatId,
      sender,
      map['text'] as String,
      timestamp,
      status: status,
      reactions: reactions,
    );
  }

  UiReaction _reaction(YamlMap map) => UiReaction(
    emoji: map['emoji'] as String,
    users: [for (final key in map['by'] as YamlList) userId(key as String)],
  );

  Duration _duration(YamlMap map) => Duration(
    days: (map['days'] as int?) ?? 0,
    hours: (map['hours'] as int?) ?? 0,
    minutes: (map['minutes'] as int?) ?? 0,
  );

  UiChatMessage _lastChatMessage(
    ChatId chatId,
    UiUserId senderId,
    String body,
  ) => UiChatMessage(
    id: (_messageIdx++).messageId(),
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

  UiChatMessage _textMessage(
    ChatId chatId,
    UiUserId senderId,
    String body,
    DateTime timestamp, {
    UiMessageStatus status = UiMessageStatus.sent,
    List<UiReaction> reactions = const [],
  }) => UiChatMessage(
    id: (_messageIdx++).messageId(),
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
    String filename,
    ImageData image,
    DateTime timestamp, {
    required AttachmentId attachmentId,
    UiMessageStatus status = UiMessageStatus.sent,
    List<UiReaction> reactions = const [],
  }) => UiChatMessage(
    id: (_messageIdx++).messageId(),
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
          content: _simpleMessage(""),
          attachments: [
            UiAttachment(
              attachmentId: attachmentId,
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
}

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
