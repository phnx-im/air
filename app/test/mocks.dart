// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/chat/chat_details.dart';
import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/message_list/message_cubit.dart';
import 'package:air/message_list/message_list_cubit.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/registration/registration.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/anchored_list/data.dart';
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
  MockUiUser({required int id, List<UiUsername> usernames = const []})
    : _userId = id.userId(),
      _usernames = usernames;

  final UiUserId _userId;
  final List<UiUsername> _usernames;

  @override
  UiUserId get userId => _userId;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;

  @override
  List<UiUsername> get usernames => _usernames;

  @override
  bool get unsupportedVersion => false;
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

class MockChatListCubit extends MockCubit<ChatListState>
    implements ChatListCubit {}

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

class MockRegistrationCubit extends MockCubit<RegistrationState>
    implements RegistrationCubit {}

class MockAttachmentsRepository extends Mock implements AttachmentsRepository {}

class MockUserSettingsCubit extends MockCubit<UserSettings>
    implements UserSettingsCubit {}

class MockChatsRepository extends Mock implements ChatsRepository {}

class MockMemberDetailsCubit extends MockCubit<MemberDetailsState>
    implements MemberDetailsCubit {}
