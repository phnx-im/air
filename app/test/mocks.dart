// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:typed_data';

import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chats_repository.dart' as chats_repository;
import 'package:air/features/chat/share_target_publisher.dart';
import 'package:air/features/chat_details/member_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/features/message_list/message_cubit.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/anchored_list/data.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:mocktail/mocktail.dart';

import 'helpers.dart';

class MockNavigationCubit extends MockCubit<NavigationState>
    implements NavigationCubit {}

class MockUserCubit extends MockCubit<UiUser> implements UserCubit {
  @override
  AppState get appState => AppState.foreground;
}

class MockUsersCubit extends MockCubit<UsersState> implements UsersCubit {}

class MockUiUser implements UiUser {
  MockUiUser({
    required int id,
    this.accountUnlinked = false,
    this.usernames = const [],
  }) : _userId = id.userId();

  final UiUserId _userId;

  @override
  UiUserId get userId => _userId;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;

  @override
  final List<UiUsername> usernames;

  @override
  bool get unsupportedVersion => false;

  @override
  final bool accountUnlinked;
}

class MockUsersState implements UsersState {
  MockUsersState({
    UiUserId? defaultUserId,
    required List<UiUserProfile> profiles,
  }) : _defaultUserId = defaultUserId ?? 1.userId(),
       _profiles = {for (final profile in profiles) profile.userId: profile};

  final UiUserId _defaultUserId;
  final Map<UiUserId, UiUserProfile> _profiles;

  @override
  UiUserProfile profile({UiUserId? userId}) {
    final id = userId ?? _defaultUserId;
    return _profiles[id]!;
  }

  @override
  String displayName({UiUserId? userId}) => profile(userId: userId).displayName;

  @override
  ImageData? profilePicture({UiUserId? userId}) =>
      profile(userId: userId).profilePicture;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;
}

class MockChatDetailsCubit extends MockCubit<ChatDetailsState>
    implements ChatDetailsCubit {}

class MockMessageListCubit implements MessageListCubit {
  MockMessageListCubit({
    List<UiChatMessage> initialMessages = const [],
    this.onJumpToMessage,
    this.onJumpToBottom,
    this.onLoadNewer,
    this.onLoadOlder,
  }) {
    _syncMessageData(initialMessages);
  }

  final StreamController<MessageListStateWrapper> _controller =
      StreamController<MessageListStateWrapper>.broadcast(sync: true);
  final StreamController<MessageListCommand> _commands =
      StreamController<MessageListCommand>.broadcast(sync: true);
  final StreamController<Set<MessageId>> _incomingMessages =
      StreamController<Set<MessageId>>.broadcast(sync: true);
  late MessageListStateWrapper _state;
  bool _isClosed = false;
  final Future<void> Function(MessageId messageId)? onJumpToMessage;
  final Future<void> Function()? onJumpToBottom;
  final Future<void> Function()? onLoadNewer;
  final Future<void> Function()? onLoadOlder;

  @override
  AnchoredListData<UiChatMessage> messageData = AnchoredListData();

  @override
  bool get isClosed => _isClosed;

  @override
  MessageListStateWrapper get state => _state;

  @override
  Stream<MessageListStateWrapper> get stream => _controller.stream;

  @override
  Stream<MessageListCommand> get commands => _commands.stream;

  @override
  Stream<Set<MessageId>> get incomingMessages => _incomingMessages.stream;

  @override
  Future<void> jumpToBottom() async {
    await onJumpToBottom?.call();
  }

  @override
  Future<void> jumpToMessage({required MessageId messageId}) async {
    await onJumpToMessage?.call(messageId);
  }

  @override
  Future<void> loadNewer() async {
    await onLoadNewer?.call();
  }

  @override
  Future<void> loadOlder() async {
    await onLoadOlder?.call();
  }

  void setState(
    List<UiChatMessage> messages, {
    bool isConnectionChat = false,
    bool hasOlder = false,
    bool hasNewer = false,
    bool isAtBottom = false,
    int? firstUnreadIndex,
    int revision = 0,
  }) {
    _syncMessageData(
      messages,
      isConnectionChat: isConnectionChat,
      hasOlder: hasOlder,
      hasNewer: hasNewer,
      isAtBottom: isAtBottom,
      firstUnreadIndex: firstUnreadIndex,
      revision: revision,
    );
    if (!_controller.isClosed) {
      _controller.add(_state);
    }
  }

  /// Splices a batch of strictly-newer messages onto the newest end of the
  /// window (index 0), mirroring a `NewerPageLoaded` pagination transition,
  /// then emits an updated state. Unlike [setState] this does not reload, so
  /// the AnchoredList sees an insert diff rather than a full reset.
  void appendNewer(
    List<UiChatMessage> newer, {
    bool hasNewer = false,
    bool isIncoming = false,
  }) {
    messageData.insertAll(0, newer.reversed.toList());
    if (isIncoming && !_incomingMessages.isClosed) {
      _incomingMessages.add(newer.map((m) => m.id).toSet());
    }
    final prev = _state.state;
    final rustState = MessageListState(
      isConnectionChat: prev.isConnectionChat ?? false,
      hasOlder: prev.hasOlder,
      hasNewer: hasNewer,
      isAtBottom: prev.isAtBottom,
      // Appending at the newest end leaves oldest-first indices unchanged.
      firstUnreadIndex: prev.firstUnreadIndex,
      revision: prev.revision + 1,
    );
    _state = MessageListStateWrapper.test(
      state: rustState,
      messageData: messageData,
      loadedMessages: messageData.items.map((m) => m.id).toSet(),
    );
    if (!_controller.isClosed) {
      _controller.add(_state);
    }
  }

  void emitCommand(MessageListCommand command) {
    if (!_commands.isClosed) {
      _commands.add(command);
    }
  }

  void _syncMessageData(
    List<UiChatMessage> messages, {
    bool isConnectionChat = false,
    bool hasOlder = false,
    bool hasNewer = false,
    bool isAtBottom = false,
    int? firstUnreadIndex,
    int revision = 0,
  }) {
    // AnchoredListData: index 0 = newest; messages is oldest-first
    final reversed = messages.reversed.toList();
    messageData.reload(reversed);
    final rustState = MessageListState(
      isConnectionChat: isConnectionChat,
      hasOlder: hasOlder,
      hasNewer: hasNewer,
      isAtBottom: isAtBottom,
      firstUnreadIndex: firstUnreadIndex,
      revision: revision,
    );
    _state = MessageListStateWrapper.test(
      state: rustState,
      messageData: messageData,
      loadedMessages: reversed.map((m) => m.id).toSet(),
    );
  }

  @override
  Future<void> close() async {
    _isClosed = true;
    messageData.dispose();
    await _commands.close();
    await _controller.close();
    await _incomingMessages.close();
  }
}

class MockMessageCubit extends MockCubit<MessageState> implements MessageCubit {
  MockMessageCubit({required MessageState initialState}) {
    when(() => state).thenReturn(initialState);
  }
}

class MockLoadableUserCubit extends MockCubit<LoadableUser>
    implements LoadableUserCubit {}

class MockUser extends Mock implements User {}

class MockMultiDeviceProvisionedUser extends Mock
    implements MultiDeviceProvisionedUser {}

class MockRegistrationCubit extends MockCubit<RegistrationState>
    implements RegistrationCubit {}

class MockAttachmentsRepository extends Mock implements AttachmentsRepository {}

class MockUserSettingsCubit extends MockCubit<UserSettings>
    implements UserSettingsCubit {}

class MockShareTargetPublisher extends Mock implements ShareTargetPublisher {}

/// A [chats_repository.ChatsRepository] serving a fixed set of chats.
class FakeChatsRepository implements chats_repository.ChatsRepository {
  FakeChatsRepository(List<UiChatDetails> chats)
    : _order = [for (final chat in chats) chat.id],
      _chats = {for (final chat in chats) chat.id: chat};

  final List<ChatId> _order;
  final Map<ChatId, UiChatDetails> _chats;

  @override
  bool get isLoaded => true;

  @override
  List<ChatId> get order => _order;

  @override
  Stream<List<ChatId>> watchOrder() => Stream.value(_order);

  @override
  UiChatDetails getChat(ChatId id) => _chats[id]!;

  @override
  Stream<Set<ChatId>> watchChanges() => const Stream.empty();

  @override
  Stream<UiChatDetails?> watchChat(ChatId id) => Stream.value(_chats[id]);

  @override
  Stream<List<UiUserId>?> watchMembers(ChatId id) => Stream.value(null);

  @override
  Future<void> mute(ChatId id, {required UiChatMuted until}) => Future.value();

  @override
  Future<void> unmute(ChatId id) => Future.value();

  @override
  Future<AddUsernameContactError?> createContactChat({
    required UiUsername username,
    required UsernameHash hash,
  }) => Future.value(null);

  @override
  Future<ChatId> createGroupChat({
    required String groupName,
    Uint8List? picture,
    required bool isApq,
  }) => throw UnimplementedError();

  @override
  Future<void> dispose() => Future.value();
}

class MockMemberDetailsCubit extends MockCubit<MemberDetailsState>
    implements MemberDetailsCubit {}
